# Plugin System

## Scope

This document records the design of the launchpi plugin system: how an
integration is compiled in, configured, instantiated, and how the values it
publishes end up as pixels on a key.

The action half builds on models that already existed. `Action`,
`ActionBinding` and `ActionTrigger` were defined before any executor did, and
this design fills that seam rather than replacing it.

The value half replaced an earlier draft. That draft had plugins expose both
named variables and separately declared boolean feedbacks; the Concepts section
explains why the two collapsed into one.

## Concepts

Four concepts, all namespaced by instance.

| Concept | Meaning |
| --- | --- |
| Plugin type | A capability compiled into the daemon, named by a `&'static str` such as `http` or `mpris`. |
| Instance | A configured copy of a type, identified as `<type>.<name>`, for example `http.weather`. |
| Action | Something an instance can do, invoked when a gesture fires. |
| Value | Anything an instance knows, published as it changes and referenced as `$(http.weather:temperature)`. |

There is deliberately no separate feedback concept. An earlier draft of this
design split values from boolean feedback queries — values carrying content,
queries carrying style — and that split turned out to be the thing standing in
the way of the most obvious request anyone makes: bind a key's colour to a
light's colour. A boolean cannot carry a colour. One kind of thing, referenced
the same way everywhere, does both jobs.

A value name is free-form and may be structured, so
`$(hass.home:light.kitchen.color)` is a single reference whose name happens to
identify an entity and an attribute. This is what keeps a large installation
tractable: an instance publishes what something is actually watching rather than
everything it can see. See Subscriptions below.

Values come from three places — plugins publish them, surfaces publish them (a
dial's position, a key's pressed state), and configuration derives them from
other values. Downstream they are all the same thing.

Actions are declared in the plugin's manifest so the web UI can offer pickers
rather than free-text fields. Values are not declared exhaustively, because a
plugin generally cannot know them ahead of time; the manifest lists the ones
worth suggesting, and a plugin may publish any name.

## Bindings

Anything a control displays is either a literal or a reference:

```toml
[panels.controls.default_state]
text             = "$(mpris.default:title)"
image            = "$(mpris.default:art)"
background_color = "$(hass.home:light.kitchen.color)"
```

A field holding a reference resolves at render time and repaints the control
whenever the referenced value changes. A field holding a literal never does.

Bare references are the whole of the binding language today. A later phase adds
operators — comparison, boolean logic, a ternary and a few functions — so a
field can choose between values:

```text
$(hass.home:light_a.state) == 'on' ? $(hass.home:light_a.color) : $(hass.home:light_b.color)
```

Derived values will hoist an expression like that under a name, so several
controls share one definition and a change trickles through it to everything
reading it. The dependency graph below already propagates that way; only the
evaluator is missing.

## Instance Identity

An instance is a file. `~/.config/launchpi/plugins/http.weather.toml` is the
instance `http.weather`, and the directory listing is the instance list. There
is no index file to keep in sync, which means a declarative user drops files
into the directory and is done.

The filename grammar is `<type>.<name>.toml`, where `name` matches
`[a-z0-9][a-z0-9-]*`. A file whose type is not in the registry loads as an
instance in the `Error` state rather than failing the daemon's boot, so an
unknown or misspelled plugin surfaces in the UI with a readable reason.

`IntegrationId` holds the composed `<type>.<name>` string. Every
`Action::InvokeIntegration` and every value reference names an instance through
it.

## The Plugin Trait

Plugins are Rust modules compiled into the daemon. There is no IPC boundary, no
WASM sandbox, and no dynamic loading. A plugin that needs D-Bus depends on
`zbus`; a plugin that needs HTTP uses the shared client handed to it.

```rust
#[async_trait]
pub trait Plugin: Send + Sync {
    async fn invoke(&self, action_name: &str, parameters: &JsonValue)
        -> Result<(), PluginError>;
    async fn subscribe(&self, _wanted: &[ValueRef])
        -> Result<(), PluginError> { Ok(()) }
    async fn shutdown(&self) {}
}

pub struct PluginFactory {
    pub plugin_type: &'static str,
    pub manifest: fn() -> PluginManifest,
    pub start: fn(InstanceConfig, PluginContext)
        -> BoxFuture<'static, Result<Arc<dyn Plugin>, PluginError>>,
}
```

`registry()` in `daemon/src/plugins/mod.rs` returns a `&'static [PluginFactory]`
built as a plain static slice. Registering a plugin is one entry there plus a
module; no registration crate is involved.

A plugin is push-only for values: when its view of the world moves it calls
`ctx.set_value`, and the engine works out what that repaints. It never pushes
render commands and is never asked to compute anything during a render, which is
what keeps the render path free of I/O.

The boundary is deliberately message-shaped: an action is a name plus JSON, a
value is a name plus a scalar, and everything a plugin publishes travels through
sinks rather than shared state. Nothing in the trait assumes the implementation
is in-process, so an out-of-process transport can later be added as a second
implementation without reshaping plugin code.

## Lifecycle

`start` receives the instance's resolved configuration and a `PluginContext`,
and returns the live handle.

```rust
pub struct PluginContext {
    pub integration_id: IntegrationId,
    pub http: reqwest::Client,      // shared across all instances
    pub assets: AssetStore,
    pub cancel: CancellationToken,
    // sinks
    // set_value(name, Value)
    // set_image(name, bytes) -> AssetId
    // log(level, message)
}
```

Long-running work — a poll loop, a D-Bus listener, a WebSocket reader — is
spawned by `start` and tied to `cancel`. Disabling an instance in the UI, or
editing its configuration, cancels the token, awaits `shutdown`, and starts a
fresh instance from the new configuration. There is no in-place reconfiguration
path, because a plugin holding a connection opened under old credentials is
harder to reason about than one that restarts.

An instance carries a status the UI mirrors from the device pages:

```text
Starting -> Running
         -> Error(reason)
Disabled
```

Configuration errors, including an unresolvable secret reference, produce
`Error` with a readable message rather than a panic or a silent no-op.

## Manifest

The manifest is what makes the web UI possible without hand-writing a form per
plugin. It declares the configuration schema, the actions, and the values worth
suggesting, and the same `ConfigField` shape drives every generated input: the
instance configuration form and the action parameter editor.

| Item | Declares |
| --- | --- |
| `ConfigField` | key, label, kind, required, placeholder, help |
| `ActionDefinition` | name, label, description, parameters |
| `ValueDefinition` | name, label, description, value kind |

`ConfigField::kind` is one of text, number, boolean, select with options, or
secret. A secret field never round-trips its value to the browser; the UI edits
which form the reference takes, not what it contains.

Declared values populate the reference picker in the button editor, so a user
inserts `$(mpris.default:title)` from a list rather than remembering the
spelling. The list is a suggestion, not a constraint — a plugin may publish any
name, and a reference to a name the manifest never mentioned resolves normally.

## Values

A value is `(IntegrationId, name)` holding text, a number, a boolean, a colour,
or an `AssetId` for an image. Instances publish through `ctx.set_value`; the
`user` namespace is reserved for `Action::SetVariable`, so an unqualified name
written by a button becomes `$(user:foo)`.

Publishing is idempotent. Setting a value to what it already holds marks nothing
dirty and repaints nothing, so a poll loop running every second against a
reading that changes hourly costs one comparison per tick.

References are written `$(instance:name)`. `$$` is a literal dollar. A reference
to an unknown value renders as empty rather than as its own source text, and a
malformed reference is left alone — an unmatched `$(` is far more likely to be
intentional text than a broken binding worth hiding. A malformed reference never
consumes a well-formed one that follows it.

The parser is hand-rolled; there is no regex dependency.

Colours are the one place where a reference and a literal look different. A
literal colour is a table (`{ red, green, blue, alpha }`); a bound colour is the
string `"$(...)"`. The daemon accepts either and a plugin publishing a colour
value is responsible for producing something a colour field can use.

## Render Path

`rendering_for_control` resolves a control against a `RenderContext`, which is a
snapshot of the value store:

```text
base   = if pressed { pressed_state ?? default_state } else { default_state }
text   = ctx.resolve_text(base.text)
image  = ctx.resolve_asset(base.image)
colors = ctx.resolve_color(base.foreground), ctx.resolve_color(base.background)
-> KeyRendering { key_index, text, image, progress, foreground, background }
```

Every field goes through the same resolution: a literal passes through, a
reference is looked up. There is no separate overlay pass, because there are no
boolean feedbacks to overlay.

`KeyRendering` grows `image` and `progress`. Both already exist on
`RenderedState` and are silently dropped by the current implementation.

### Reverse Index

The engine keeps a map from each value to the keys that read it:

```text
ValueRef -> {(SurfaceId, key_index)}
```

They are rebuilt whenever the set of visible controls changes: `upsert_panel`,
`remove_panel`, `assign_active_panel`, and device enable or disable. Those are
the same four call sites that already trigger a full `render_active_panel`.

### Subscriptions

Rebuilding the index also yields, per instance, the set of value names anything
is actually watching. That set is pushed to `Plugin::subscribe`. It is how
`mpris` learns that nobody wants track art and can skip fetching it, and how
`hass` learns which entities to watch instead of mirroring an entire
installation. An instance with no subscribers stays running but is free to idle.

Because a value name is free-form, a subscription is the plugin's own vocabulary
coming back to it: `hass` receives `light.kitchen.color` and knows exactly which
entity and attribute that means. Nothing in the daemon parses those names.

### Dirty Dispatch

Value changes land in a coalescing queue drained on a short tick, currently
50 ms. Each drain unions the dirty key sets and re-resolves only those keys. Everything downstream is unchanged: the bounded per-surface
`mpsc<SurfaceCommand>`, the `PendingRenders` coalescing in
`daemon/src/streamdeck/studio.rs`, and the write loop.

A tick matters because update rates differ by two orders of magnitude. An
`mpris` position value ticks once a second; a Home Assistant state firehose does
not pace itself at all.

### Deduplication

Every dispatch currently re-encodes a 96 by 96 JPEG unconditionally. That is
acceptable when repaints only follow a key press, and is not acceptable once a
value can change on its own. The registry keeps a hash of the last resolved
`KeyRendering` per `(surface, key)` and drops an identical resolution before it
reaches the queue.

This is a prerequisite of the feature, not an optimization of it. Without it a
single one-hertz value on a 16 by 2 Studio would re-encode thirty-two images a
second to change one of them.

## Action Execution

`record_key_state` holds a `std::sync::RwLock` write guard for its whole body,
so no async work can happen inline. The registry gains a dedicated bounded
`mpsc::Sender<InputEvent>` and `try_send`s to it, warning on a full queue in the
same shape as the existing `dispatch`.

This is deliberately not the existing `ServerEvent` broadcast, which drops
messages when a receiver lags. A dropped render is cosmetic; a dropped action is
a light that did not turn on.

The engine's input task then:

1. Resolves `(surface_id, key_index)` to the active panel's `Control`, through
   the same lookup `rendering_for_key` uses, lifted into a shared helper.
2. Matches the event against each binding's `ActionTrigger`. `Press` and
   `Release` fire directly. `Hold { duration_ms }` spawns a timer task keyed by
   `(surface, control)` whose handle is aborted on release.
   `RotateClockwise` and `RotateCounterClockwise` hang off `record_dial_turn`.
3. Spawns one task per fired binding and runs its `Vec<Action>` sequentially,
   so `Wait { duration_ms }` means what it says.
4. Dispatches per variant: `InvokeIntegration` to `Plugin::invoke`,
   `SetVariable` into the `user` namespace, `ChangePanel` to
   `assign_active_panel`, and `Wait` to a sleep.
5. Logs failures to both the surface log and the instance log and emits an
   event the UI can show. A failing action does not abort the rest of its chain.

## Assets

`AssetId` gains a grammar:

```text
builtin:<shape>     one of the shapes already drawn by draw_icon
file:<path>         a path on disk
hash:<sha256>       an entry in the content-addressed cache
```

`AssetStore` caches bytes at `~/.cache/launchpi/assets/<aa>/<hash>` and keeps an
in-memory LRU of already decoded and resized buffers, so the render path never
decodes the same album art twice. Plugins fetch remote images themselves through
`ctx.http` and call `ctx.set_image`, which stores the bytes and publishes the
resulting `AssetId` as a value.

The renderer draws the image layer first, scaled to cover and centre-cropped,
then the icon, then text, then a progress bar along the bottom edge when
`progress` is set. Existing alpha blending, text auto-fit and the Stream Deck XL
180-degree flip are untouched.

Because the web preview renders through `POST /api/render-key`, which calls the
same function, the browser preview stays pixel-identical to the hardware for
free.

## Configuration Format Changes

`Action` and `ActionTrigger` are externally-tagged PascalCase enums, which
produce awkward TOML. They gain `#[serde(tag = "type", rename_all =
"snake_case")]` and `#[serde(rename_all = "snake_case")]` respectively:

```toml
[[controls.action_bindings]]
gesture = "press"

[[controls.action_bindings.actions]]
type = "invoke_integration"
integration_id = "hass.home"
action_name = "light.toggle"
parameters = { entity_id = "light.kitchen" }
```

This changes the wire format, but every `action_bindings` written so far is
empty, so there is nothing to migrate. `PanelsDocument.version` becomes `2`, and
the loader accepts `1` or `2` where it previously hard-failed on anything but
`1`.

See `configuration.md` for the full schema, including the secret reference forms
and what the copy-TOML buttons emit.

## References

- [bitfocus/companion](https://github.com/bitfocus/companion), the source of the
  action and value vocabulary this design follows
- [MPRIS D-Bus Interface Specification](https://specifications.freedesktop.org/mpris-spec/latest/)
- [Home Assistant WebSocket API](https://developers.home-assistant.io/docs/api/websocket)
