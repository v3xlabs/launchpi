# Plugin System

## Scope

This document records the design of the launchpi plugin system: how an
integration is compiled in, configured, instantiated, and how the values it
publishes end up as pixels on a key.

The models it builds on already exist. `Action`, `ActionBinding`,
`ActionTrigger`, `Feedback` and `FeedbackBinding` were defined in
`daemon/src/models/action.rs` and `daemon/src/models/feedback.rs` before any
executor existed. They serialize, they round-trip through `panels.toml`, and
until now nothing ever read them. This design fills that seam rather than
replacing it.

## Concepts

Five concepts, all namespaced by instance.

| Concept | Meaning |
| --- | --- |
| Plugin type | A capability compiled into the daemon, named by a `&'static str` such as `http` or `mpris`. |
| Instance | A configured copy of a type, identified as `<type>.<name>`, for example `http.weather`. |
| Action | Something an instance can do, invoked when a gesture fires. |
| Variable | A named value an instance publishes, referenced as `$(http.weather:temperature)`. |
| Feedback | A declared boolean query with parameters that overlays a style onto a button. |

Variables carry content and feedbacks carry style. The split is deliberate.
Showing a song title is interpolation into text; turning a key amber because a
light is on is a style overlay. Collapsing the two would force every dynamic
label through a declared query with parameters, and collapsing them the other
way would require an expression language before the first plugin could ship.

Both are declared in the plugin's manifest, so the web UI offers pickers rather
than free-text fields, and a typo becomes a validation error instead of a blank
button.

## Instance Identity

An instance is a file. `~/.config/launchpi/plugins/http.weather.toml` is the
instance `http.weather`, and the directory listing is the instance list. There
is no index file to keep in sync, which means a declarative user drops files
into the directory and is done.

The filename grammar is `<type>.<name>.toml`, where `name` matches
`[a-z0-9][a-z0-9-]*`. A file whose type is not in the registry loads as an
instance in the `Error` state rather than failing the daemon's boot, so an
unknown or misspelled plugin surfaces in the UI with a readable reason.

`IntegrationId`, already defined in `daemon/src/models/identifiers.rs`, holds
the composed `<type>.<name>` string. Every `Action::InvokeIntegration` and
every `Feedback` references an instance through it.

## The Plugin Trait

Plugins are Rust modules compiled into the daemon. There is no IPC boundary, no
WASM sandbox, and no dynamic loading. A plugin that needs D-Bus depends on
`zbus`; a plugin that needs HTTP uses the shared client handed to it.

```rust
#[async_trait]
pub trait Plugin: Send + Sync {
    async fn invoke(&self, action_name: &str, parameters: &JsonValue)
        -> Result<(), PluginError>;
    async fn evaluate(&self, feedback_name: &str, parameters: &JsonValue)
        -> Result<bool, PluginError>;
    async fn subscribe(&self, _subscriptions: &[Subscription])
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

`evaluate` is pull-based and is expected to be cheap and in-memory. The plugin
keeps its own view of the world and the engine asks it questions. When that view
changes the plugin calls `ctx.invalidate_feedbacks()`, and the engine
re-evaluates only the feedback instances that actually have subscribers, then
diffs the results. A plugin never pushes render commands.

The boundary is deliberately message-shaped: an action is a name plus JSON, a
feedback is a name plus JSON returning a bool, and everything a plugin publishes
travels through sinks rather than through shared state. Nothing in the trait
assumes the implementation is in-process, so an out-of-process transport can
later be added as a second implementation without reshaping plugin code.

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
    // set_variable(name, VariableValue)
    // set_image(name, bytes) -> AssetId
    // invalidate_feedbacks()
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
plugin. It declares the configuration schema, the actions, the feedbacks and
the variables, and the same `ConfigField` shape drives every generated input:
the instance configuration form, the action parameter editor, and the feedback
parameter editor.

| Item | Declares |
| --- | --- |
| `ConfigField` | key, label, kind, required, placeholder, help |
| `ActionDefinition` | name, label, description, parameters |
| `FeedbackDefinition` | name, label, description, parameters |
| `VariableDefinition` | name, label, description, value kind |

`ConfigField::kind` is one of text, number, boolean, select with options, or
secret. A secret field never round-trips its value to the browser; the UI edits
which form the reference takes, not what it contains.

Declared variables are also what populates the variable picker in the button
editor, so a user inserts `$(mpris.default:title)` from a list rather than
remembering the spelling.

## Variables

A variable is `(IntegrationId, name)` holding a `VariableValue`, which is text,
a number, a boolean, or an `AssetId` for an image. Instances publish through
`ctx.set_variable`; the `user` namespace is reserved for `Action::SetVariable`,
so an unqualified variable name written by a button becomes `$(user:foo)`.

References are written `$(instance:name)` and are interpolated into any
`RenderedState.text` and into `RenderedState.image`. `$$` is a literal dollar.
A reference to an unknown variable renders as empty rather than as its own
source text, and a malformed reference is left alone — an unmatched `$(` is far
more likely to be intentional text than a broken binding worth hiding.

The parser is hand-rolled; there is no regex dependency.

## Feedbacks

A `FeedbackBinding` pairs a `Feedback` — instance, name, parameters — with a
`RenderedStateOverride`. When the query evaluates true the override is applied
field by field on top of the current state. Bindings apply in declaration
order, so a later binding wins on any field it sets and contributes nothing on
the fields it leaves as `None`.

Feedback results are cached by `FeedbackKey`, which is the instance, the
feedback name, and a hash of the canonicalized parameter JSON. Two buttons
watching the same light share one cache entry and one evaluation.

## Render Path

`rendering_for_control` in `daemon/src/state.rs` is a pure function today with
no notion of live state. It gains a `&RenderContext` — a variable snapshot plus
the feedback cache — and resolves in this order:

```text
base   = if pressed { pressed_state ?? default_state } else { default_state }
for binding in control.feedback_bindings, in order:
    if ctx.feedback(&binding.feedback) == Some(true):
        base = base.overlay(&binding.state)
text   = ctx.interpolate(base.text)
image  = ctx.resolve_asset(base.image)
-> KeyRendering { key_index, text, image, progress, foreground, background }
```

`KeyRendering` grows `image` and `progress`. Both already exist on
`RenderedState` and are silently dropped by the current implementation.

### Reverse Index

The engine keeps two maps from what a button depends on to the keys that depend
on it:

```text
VariableRef -> {(SurfaceId, key_index)}
FeedbackKey -> {(SurfaceId, key_index)}
```

They are rebuilt whenever the set of visible controls changes: `upsert_panel`,
`remove_panel`, `assign_active_panel`, and device enable or disable. Those are
the same four call sites that already trigger a full `render_active_panel`.

### Subscriptions

Rebuilding the index also yields, per instance, the set of variables and
feedback instances anything is actually watching. That set is pushed to
`Plugin::subscribe`. It is how `mpris` learns that nobody wants track art and
can skip fetching it, and how `hass` learns which entities to watch instead of
mirroring an entire installation. An instance with no subscribers stays running
but is free to idle.

### Dirty Dispatch

Variable and feedback changes land in a coalescing queue drained on a short
tick, currently 50 ms. Each drain unions the dirty key sets and re-resolves
only those keys. Everything downstream is unchanged: the bounded per-surface
`mpsc<SurfaceCommand>`, the `PendingRenders` coalescing in
`daemon/src/streamdeck/studio.rs`, and the write loop.

A tick matters because update rates differ by two orders of magnitude. An
`mpris` position variable ticks once a second; a Home Assistant state firehose
does not pace itself at all.

### Deduplication

Every dispatch currently re-encodes a 96 by 96 JPEG unconditionally. That is
acceptable when repaints only follow a key press, and is not acceptable once a
variable can change on its own. The registry keeps a hash of the last resolved
`KeyRendering` per `(surface, key)` and drops an identical resolution before it
reaches the queue.

This is a prerequisite of the feature, not an optimization of it. Without it a
single one-hertz variable on a 16 by 2 Studio would re-encode thirty-two images
a second to change one of them.

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
resulting `AssetId` as a variable.

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
  action, feedback and variable vocabulary this design follows
- [MPRIS D-Bus Interface Specification](https://specifications.freedesktop.org/mpris-spec/latest/)
- [Home Assistant WebSocket API](https://developers.home-assistant.io/docs/api/websocket)
