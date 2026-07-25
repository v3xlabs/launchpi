# Writing a Plugin

## Scope

This document walks through adding a plugin type to launchpi, using the `http`
plugin as the worked example, and documents the configuration each built-in
plugin accepts.

Read `plugins.md` first for the design. This is the practical companion to it.

## What a plugin owes the daemon

Four things:

1. A `PluginManifest` describing what it can be configured with, what it can
   do, what it can be asked, and what it publishes.
2. A `start` function that validates configuration, spawns whatever long-lived
   work it needs, and returns a handle.
3. `invoke`, which performs a named action.
4. `evaluate`, which answers a named boolean question.

Everything else is optional. A plugin that publishes no variables never touches
a sink; a plugin that has no queryable state returns
`PluginError::UnknownFeedback` from `evaluate`.

## Layout

```text
daemon/src/plugins/builtin/http/
  mod.rs        manifest, factory, Plugin impl
  config.rs     the deserialized [config] table
  poll.rs       the poll loop
```

Registration is one entry in `daemon/src/plugins/mod.rs`:

```rust
pub fn registry() -> &'static [PluginFactory] {
    &[
        builtin::http::FACTORY,
        builtin::mpris::FACTORY,
        builtin::hass::FACTORY,
        builtin::spotify::FACTORY,
    ]
}
```

There is no registration macro and no inventory crate. Adding a plugin is a
module and a line.

## The manifest

The manifest is what makes the web UI work without a hand-written form per
plugin. The same `ConfigField` shape describes instance configuration, action
parameters and feedback parameters, and the UI renders all three through one
component.

```rust
fn manifest() -> PluginManifest {
    PluginManifest {
        plugin_type: "http",
        display_name: "HTTP",
        description: "Call HTTP endpoints and publish values from their responses.",
        config_schema: vec![
            ConfigField::text("base_url")
                .label("Base URL")
                .placeholder("https://api.example.com")
                .required(),
            ConfigField::number("timeout_ms").label("Timeout (ms)"),
            ConfigField::secret("api_key").label("API key"),
        ],
        actions: vec![
            ActionDefinition::new("request")
                .label("Send request")
                .parameters(vec![
                    ConfigField::select("method")
                        .options(["GET", "POST", "PUT", "PATCH", "DELETE"])
                        .required(),
                    ConfigField::text("path").required(),
                    ConfigField::text("body"),
                ]),
        ],
        feedbacks: vec![
            FeedbackDefinition::new("value_equals")
                .label("Value equals")
                .parameters(vec![
                    ConfigField::text("variable").required(),
                    ConfigField::text("value").required(),
                ]),
        ],
        variables: vec![],
    }
}
```

`variables` is empty here because `http` publishes whatever its poll entries
declare, which is only known once an instance is configured. A manifest may
declare a static variable list, and a plugin may also publish variables the
manifest never mentioned — the manifest drives the picker, not validation.

Anything a user types that should not be echoed back to the browser must be
declared `ConfigField::secret`. That is the only signal the export path and the
UI have.

## Start

```rust
async fn start(config: InstanceConfig, ctx: PluginContext)
    -> Result<Arc<dyn Plugin>, PluginError>
{
    let settings: HttpConfig = config.deserialize()?;
    let api_key = config.secret("api_key")?;   // resolves env / file / inline

    let plugin = Arc::new(HttpPlugin::new(settings, api_key, ctx.clone()));

    for entry in plugin.polls() {
        let plugin = plugin.clone();
        let ctx = ctx.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(entry.interval);
            loop {
                tokio::select! {
                    _ = ctx.cancel.cancelled() => break,
                    _ = ticker.tick() => plugin.run_poll(&entry).await,
                }
            }
        });
    }

    Ok(plugin)
}
```

Three rules for `start`:

- Validate configuration here and return `PluginError`. A bad configuration
  should put the instance in the error state with a readable message, not fail
  later at the first key press.
- Resolve secrets here, once. A secret is read at start and held; it is not
  re-read per request.
- Tie every spawned task to `ctx.cancel`. Disabling an instance or editing its
  configuration cancels the token, awaits `shutdown`, and starts a fresh
  instance. A task that ignores the token outlives its plugin.

## Publishing variables

```rust
ctx.set_variable("temperature", VariableValue::Number(21.4));
ctx.set_variable("status", VariableValue::Text("ok".into()));
```

The engine deduplicates: setting a variable to the value it already holds marks
nothing dirty and re-renders nothing. A poll loop that fires every second on a
value that changes hourly costs one comparison per tick.

Images go through the asset store, which content-addresses the bytes and
returns the id to publish:

```rust
let asset = ctx.set_image("art", &bytes).await?;
```

Re-publishing identical bytes yields the same `AssetId`, so a player that
re-announces the same album art repeatedly causes no re-render and no re-decode.

## Answering feedbacks

`evaluate` is called from the render path and must be cheap. Do not perform I/O
in it. Keep the plugin's own view of the world in memory, answer from that, and
call `ctx.invalidate_feedbacks()` when the view changes:

```rust
async fn evaluate(&self, name: &str, parameters: &JsonValue)
    -> Result<bool, PluginError>
{
    match name {
        "value_equals" => {
            let variable = parameters.str("variable")?;
            let expected = parameters.str("value")?;
            Ok(self.values.read().get(variable).is_some_and(|v| v == expected))
        }
        _ => Err(PluginError::UnknownFeedback(name.to_string())),
    }
}
```

The engine caches results keyed by the instance, the feedback name and a hash of
the canonicalized parameters, so two buttons watching the same thing evaluate
once.

## Subscriptions

`subscribe` is how a plugin learns what is actually on screen. It receives the
full current set every time it changes, not a delta, so the implementation is a
replace rather than a merge.

```rust
async fn subscribe(&self, subscriptions: &[Subscription])
    -> Result<(), PluginError>
{
    let wanted: HashSet<_> = subscriptions
        .iter()
        .filter_map(|s| s.entity_id())
        .collect();
    self.watch(wanted).await
}
```

Implementing it is optional and matters most for plugins whose upstream is
expensive or chatty. `mpris` uses it to skip fetching album art nobody is
showing; `hass` will use it to watch a handful of entities rather than mirror an
entire installation. `http` ignores it, because its work is defined by its poll
configuration rather than by what is on screen.

## Errors

```rust
pub enum PluginError {
    Configuration(String),
    UnknownAction(String),
    UnknownFeedback(String),
    Upstream(String),
    NotImplemented,
}
```

`Configuration` at start puts the instance in the error state. The rest are
per-call: they log to both the instance log and the surface log of whichever
device triggered the action, and they do not abort the remaining actions in the
binding's chain.

## Testing

A plugin's logic should be testable without its transport. Keep the parsing and
the decision-making in functions that take data, and keep the socket in the task
that calls them. The value-extraction path in `http` and the metadata mapping in
`mpris` are both plain functions over deserialized input, tested directly.

For an end-to-end check, `http` needs nothing but `python -m http.server` in a
directory containing a JSON file.

## Built-in plugins

### http

Calls HTTP endpoints and publishes values extracted from JSON responses. The
reference implementation, and the one that needs no external service to test.

```toml
version = 1
enabled = true

[config]
base_url = "https://api.open-meteo.com"
timeout_ms = 5000
api_key = { env = "LAUNCHPI_WEATHER_KEY" }

[[config.poll]]
name = "temperature"
path = "/v1/forecast?latitude=52.37&longitude=4.89&current=temperature_2m"
interval_ms = 60000
extract = "current.temperature_2m"
```

| Actions | `request` |
| --- | --- |
| Feedbacks | `status_matches`, `value_equals`, `value_above`, `value_below` |
| Variables | one per `[[config.poll]]` entry, named by its `name` |

`extract` is a dotted path into the JSON response. Method, path, headers and
body all interpolate `$(...)` references, so one instance can serve several
buttons that differ only in their parameters.

### mpris

Watches the D-Bus session bus for MPRIS2 players and tracks the active one. No
configuration is required and no credentials are involved, which makes it the
best demonstration of push-driven re-render and of album art.

```toml
version = 1
enabled = true

[config]
preferred_player = "spotify"   # optional; otherwise the most recently active
```

| Actions | `play_pause`, `next`, `previous`, `stop`, `seek`, `set_volume`, `raise` |
| --- | --- |
| Feedbacks | `is_playing`, `is_paused`, `player_is` |
| Variables | `title`, `artist`, `album`, `status`, `position`, `length`, `volume`, `art` |

`art` is an `AssetId`. The plugin fetches the artwork the player advertises,
stores it, and publishes the id, so a button can set
`image = "$(mpris.default:art)"`.

Note that `mpris` already answers "what is the local Spotify client playing"
without any Spotify credentials.

### hass

Home Assistant over its WebSocket API. Scaffolded: the module, manifest and
configuration shape exist, and the protocol work is marked `TODO`.

```toml
version = 1
enabled = true

[config]
url = "http://homeassistant.local:8123"
token = { env = "LAUNCHPI_HASS_TOKEN" }
```

| Actions | `call_service`, `light.toggle`, `light.turn_on`, `light.turn_off`, `light.set_color` |
| --- | --- |
| Feedbacks | `state_is`, `attribute_equals` |
| Variables | one per subscribed entity state and attribute |

### spotify

Spotify Web API. Scaffolded: the configuration shape and manifest exist, and the
OAuth authorization-code and refresh flow is marked `TODO`. That flow is a
subsystem in its own right — a redirect listener, token storage and refresh
scheduling — which is why it is sequenced last.

```toml
version = 1
enabled = true

[config]
client_id = "..."
client_secret = { env = "LAUNCHPI_SPOTIFY_SECRET" }
refresh_token = { file = "/run/agenix/spotify-refresh" }
```

| Actions | `play`, `pause`, `next`, `previous`, `seek`, `set_volume`, `transfer_playback` |
| --- | --- |
| Feedbacks | `is_playing`, `device_is` |
| Variables | `title`, `artist`, `album`, `art`, `device`, `progress`, `duration` |

## References

- `plugins.md` for the design and the render path
- `configuration.md` for the file schemas and secret handling
- [MPRIS D-Bus Interface Specification](https://specifications.freedesktop.org/mpris-spec/latest/)
- [Home Assistant WebSocket API](https://developers.home-assistant.io/docs/api/websocket)
- [Spotify Web API authorization](https://developer.spotify.com/documentation/web-api/tutorials/code-flow)
