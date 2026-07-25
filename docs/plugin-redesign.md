# Plugin System Redesign

## Scope

This document takes what `companion-research.md` records about Bitfocus
Companion and turns it into decisions for launchpi. It has three parts:

1. What launchpi should change, and why, ordered by leverage.
2. What Companion would look like if it had been written in Rust under
   launchpi's constraints — which is a way of separating Companion's genuine
   design mistakes from the ones its environment forced on it.
3. **External Companion as a plugin**: consuming a running Companion instance
   and its integrations from inside launchpi.

Nothing here is implemented. `plugins.md` remains the current design of record
until any of this lands; where the two disagree, this document is the newer
thinking and `plugins.md` should be updated when the corresponding change is
made.

## Constraints, Theirs and Ours

Most of Companion's awkward parts are not mistakes. They are the shape its
environment pressed into it, and the first job of this document is to say which
is which.

| Companion's constraint | What it forced |
| --- | --- |
| Integrations written by a community of AV technicians, in JavaScript | npm distribution, therefore a process boundary, therefore a wire protocol, therefore everything serialised |
| Hundreds of third-party modules that must keep working | Two API lines running simultaneously; `advanced` feedbacks alive years after being recognised as wrong |
| Cross-platform Electron desktop app | Bundled per-platform Node runtimes, Skia canvas, a worker pool |
| Live production, mixed-skill operators | Reliability over elegance; respawn-and-carry-on; no hard failures |
| Pages of buttons, many surfaces | A control addressed by `page/row/column`, and a page model surfaces bind to |

launchpi's constraints are different in every row:

| launchpi's constraint | What it permits |
| --- | --- |
| Plugins compiled into a Rust daemon | No serialisation between engine and plugin; a trait, not a protocol |
| Configuration is user-authored files, possibly Nix-generated and read-only | Files are the source of truth; migration cannot be in-place rewriting |
| No third-party ecosystem yet | Freedom to break the plugin API until there is one |
| Small surfaces — 32 keys on a Studio, 64 pads on a Launchpad | An exact reverse index is affordable; whole-panel work is cheap |
| Single operator, self-hosted | Fewer states to design for; the user can read a log |

The consequence worth internalising: **almost everything Companion pays for at
the plugin boundary, launchpi gets for free.** A colour is an `RgbaColor`, not a
number that a JS module hopes the host will interpret. An image is an
`Arc<[u8]>`, not base64 inside EJSON. Cancellation is a `CancelToken` in the
signature, not an `AbortSignal` retrofitted in a major version. The design
should spend that surplus rather than reproduce Companion's compromises.

## Part 1 — What launchpi Should Change

### 1. Definitions become runtime state, not compile-time constants

This is the highest-leverage change in the document, and several others depend
on it.

Today `PluginFactory.manifest` is `fn() -> PluginManifest`, and
`PluginManifest` carries `&'static str` fields plus
`ConfigFieldKind::Select { options: Vec<SelectOption> }`. Every action, every
value, and every dropdown choice a plugin can ever offer is fixed at compile
time.

Companion does not work this way, and the reason is decisive: **`manifest.json`
carries no capability schema at all.** Actions, feedbacks, variables and presets
are pushed at runtime by the running module, normally from `init()`, and re-sent
whenever the module learns more. That is why a Companion ATEM module can offer
you a dropdown of the switcher's actual inputs, and why an OBS module can offer
your actual scene names.

launchpi cannot do this, and `plan.md` names the case it breaks: *"the
homeassistant integration allows for hooking up any light and connecting it."*
A Home Assistant instance's entities are only knowable once the instance has
connected. With a static manifest, the entity field is free text and the user
gets to type `light.kitchn` and find out later.

The change splits the manifest in two:

```rust
pub struct PluginManifest {
    pub plugin_type: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub config_schema: Vec<ConfigField>,
}
```

Identity and the configuration form only — the things needed *before* an
instance exists, to render the "add instance" dialog. Everything else is
published by the running instance:

```rust
impl PluginContext {
    pub fn set_definitions(&self, definitions: Definitions);
}

pub struct Definitions {
    pub actions: Vec<ActionDefinition>,
    pub values: Vec<ValueDefinition>,
    pub presets: Vec<Preset>,
}
```

Replace-not-merge, like `subscribe`, for the same reason: an instance that
reconnects to a device with fewer inputs must be able to remove choices, and a
merge semantics cannot express removal.

The engine holds definitions per `IntegrationId` alongside the running handle,
emits `ServerEvent::DefinitionsChanged { integration_id }`, and the API serves
them per instance rather than per type. The UI must then handle "this instance
has no definitions yet because it has not connected", which is what
`PluginInstanceStatus::Starting` is for — a variant that currently exists and is
never assigned.

The cost is real: definitions become mutable state that the browser must track,
and `BindingsEditor` can no longer assume an action list exists for a plugin
type. The benefit is that the flagship use case works, and that every
out-of-process plugin becomes expressible — a foreign integration publishes its
definitions at runtime by construction. Part 3 does not work without this
change.

### 2. Presets

launchpi has no equivalent, and this is the largest gap between the two systems
in terms of what a user experiences.

A Companion module ships hundreds of ready-made buttons — "ATEM: Program 1",
"OBS: Toggle Scene" — complete with style, actions, and the feedbacks that make
them light up. The user drags one onto a page and it works. Nothing else in
either system does that job, and it is the single largest reason Companion is
usable rather than merely capable.

In launchpi terms a preset is a named, categorised `Control` template:

```rust
pub struct Preset {
    pub category: String,
    pub name: String,
    pub control: Control,
}
```

The one subtlety is identity. A preset is authored by the plugin *type* and does
not know which instance it will belong to, so its value references and
`InvokeIntegration` actions cannot name an `IntegrationId`. Companion solves
this by substituting the connection id when the preset is dropped. launchpi
should do the same with a `self` sigil: a preset writes
`$(self:light.kitchen.state)` and `integration_id = "self"`, and the engine
rewrites `self` to the owning instance at drop time. This keeps presets pure
data and keeps the substitution in one place.

Presets ride on change 1 — they are part of `Definitions`, published at runtime,
because a preset per discovered light is exactly what a Home Assistant
integration should be offering.

### 3. Resolve the feedback-versus-value divergence

`plugins.md` states there is *"deliberately no separate feedback concept"* and
argues that a boolean cannot carry a colour. `plugin-authoring.md` and the code
both still have `Plugin::evaluate -> bool`, `FeedbackDefinition`,
`FeedbackCache`, `FeedbackBinding`, and an overlay pass in
`RenderContext::resolve`. The code is the earlier draft that `plugins.md` says
was replaced. This must be settled before anything else touches the render path.

**Companion's own trajectory settles it.** They added a `value` feedback type in
1.13 that returns arbitrary JSON rather than a boolean. They moved variable
resolution out of modules entirely, deprecating `parseVariablesInString` in 1.13
and deleting it in 2.0. They now document `advanced` feedbacks — the only kind
that can carry a colour — as *"discouraged… does not fit into our graphics
model… will likely be removed in a future major version."* Every one of those
moves is toward "the plugin publishes typed data, the host decides what it
looks like". They cannot finish the journey because of the ecosystem. launchpi
can start there.

But values-only is strictly weaker than boolean-feedback-plus-`defaultStyle`
until an expression evaluator exists, and it discards something Companion has
that is genuinely good: the plugin *suggesting* a sensible style for a
condition. The synthesis keeps both properties:

- **Values carry everything.** Text, number, boolean, colour, image. Plugins
  publish values and nothing else. `Plugin::evaluate`, `FeedbackCache`,
  `FeedbackKey` and `FeedbackBinding` are deleted.
- **Controls gain style rules.** A rule is a condition expression plus a
  `RenderedStateOverride`, evaluated by the engine against the value store at
  render time. This is Companion's boolean feedback with the predicate moved
  from the plugin into the host.
- **Definitions may ship suggested rules** alongside a value definition. That is
  `defaultStyle`'s job, and it is what makes a value discoverable rather than
  merely available.

What this buys, beyond the colour case: no call into a plugin during a render,
no cache to invalidate, no `evaluate` that must be cheap and must not block, and
no feedback-recheck storms — the entire apparatus Companion had to build
(`#feedbacksBeingChecked`, `needsRecheck`, the 5/25 ms debounce, `AbortSignal`,
the `abortable` starvation guard) simply has no analogue, because the engine
already holds every input to the decision.

Sequencing matters: the expression evaluator lands first, then style rules, then
feedbacks are deleted. Doing it in the other order leaves a period with no way
to express "yellow when the light is on".

### 4. Lazy, resolution-independent render handles

Steal `ImageResult` outright. It is the best idea in Companion's codebase.

Today `rendering_for_control` produces a `KeyRendering`, and each surface
encodes its own JPEG; the browser preview goes through `POST /api/render-key`
and rasterises separately. With a Studio, a Stream Deck XL, and an open browser
tab all showing the same panel, the same button is rasterised three times from
three code paths.

Companion separates the two ideas completely. A render produces an `ImageResult`
— a lazy handle carrying a content `cacheKey` and nothing else — and each
consumer then calls `drawNative(width, height, rotation, format)`, memoised per
`${w}x${h}-${rot}-${fmt}`. One logical render feeds a 72 px Stream Deck, a
288 px web preview and a Satellite client at whatever size it asked for.

The Rust shape:

```rust
pub struct Rendering { /* resolved, device-independent: text, asset, colours, progress */ }

pub struct RasterCache { /* keyed by (content_hash, width, height, rotation, format) */ }
```

`RenderLedger` already hashes `KeyRendering` to suppress identical repaints;
that hash becomes the content identity, so the two mechanisms merge rather than
duplicate. This also deletes the web-preview special case: the browser becomes
one more consumer asking for a different size, and `plugins.md`'s promise that
*"the browser preview stays pixel-identical to the hardware for free"* becomes
structural rather than a convention two call sites have to maintain.

### 5. Close the dead-code gaps

Several things are declared and never reachable. Each is small; together they
are the difference between a design and a working system.

- **`InputEvent` has only a `Key` variant.** `record_dial_turn` and
  `record_dial_press` never call `dispatch_input`, so
  `ActionTrigger::RotateClockwise`, `RotateCounterClockwise` and `ValueChanged`
  can never fire. A dial on a Studio cannot trigger an action today.
- **`PluginInstanceStatus::Starting` is never assigned**, because
  `start_instance` awaits the factory synchronously. Change 1 makes this matter:
  an instance that has started but not yet published definitions is exactly
  `Starting`.
- **`RenderedState.progress` is resolved and then dropped**, and `KeyRendering`
  has no `image` field, so `VariableValue::Image` can never reach a device.
- **`EngineSignal::InstanceLog` is consumed by a bare `debug!`.** A plugin that
  cannot tell the user why it failed is a plugin the user cannot fix. This needs
  a bounded per-instance ring buffer, an API route, and a panel in the instance
  detail page.

### 6. A richer instance status vocabulary

Companion has eight states, and the distinction is not decorative — the UI
routes the user to a different remedy for each. `BadConfig` means fix the form;
`AuthenticationFailure` means fix the token; `ConnectionFailure` means check the
network. launchpi collapses all three into `Error { reason: String }` and asks
the user to read the string.

Part 3 adds a case launchpi has no vocabulary for at all: an instance that is
working, but with reduced capability — the Companion bridge running on the
stable HTTP API because the enrichment layer is unavailable. That wants a
`Degraded { reason }` state that is neither `Running` nor `Error`.

### 7. Actions should be able to return something

`Plugin::invoke` returns `Result<(), PluginError>`. An action cannot report a
result, so "run this, then use its output" is inexpressible.

Companion added action return values in 1.14 and a full action result store in
2.0, which is evidence that the need is real and that retrofitting it is a
breaking change. Widening to `Result<JsonValue, PluginError>` now costs nothing
and avoids paying for it later.

### 8. Config migration must not rewrite the user's files

Companion migrates connection configuration in place, through ordered upgrade
scripts and a persisted per-connection `upgradeIndex`. That is correct for
Companion, whose configuration lives in a database it owns.

It is wrong for launchpi. An instance file may have been hand-written, may be
generated by Nix, and may be read-only on disk. Silently rewriting it is hostile
and, in the Nix case, futile — the change is reverted on the next rebuild.

The right model inverts it: **migrate on read, never write back unless asked.**
A document at an older `version` is upgraded in memory, the instance runs
normally, and the UI reports that the file uses an older schema and offers the
migrated TOML through the copy-TOML affordance that already exists. The
declarative user pastes it into their Nix; the imperative user clicks save. This
is a case where launchpi's constraint produces a *better* answer than
Companion's, and it should be written down as a rule before the first schema
change makes it urgent.

### 9. What not to copy

**Do not adopt render-discovers-dependencies.** Companion keeps no reverse
index; `ControlsController.onVariablesChanged` iterates every control and each
one rejects cheaply with `#lastDrawVariables.isDisjointFrom(changed)`, where
`#lastDrawVariables` was captured during the last render. It is elegant, and it
is the right answer *for them*, because their dependencies were historically
discovered inside plugin callbacks and they still have expressions, local
variables and cross-button references to contend with.

launchpi's `DependencyIndex` is built by statically scanning declarative
bindings, which is cheaper and exact. It stays correct when expressions land, so
long as expressions are statically analysable: parse the expression, walk the
AST, collect references. **The rule that preserves this is to forbid
indirection** — no `$($(x))`, no computing a reference name at runtime. That is
a small price for keeping an exact index and never shipping the class of bug
where the index disagrees with reality.

**Do not adopt advanced feedbacks in any form**, including a well-intentioned
"plugin returns a style override" escape hatch. Companion's experience is
unambiguous: it defeats render caching, defeats user style overrides, defeats
layered graphics, and had to be retrofitted with `affectedProperties` to make
invalidation tractable. A plugin publishes data; the host decides pixels.

**Do not keep the value store publicly writable.** `VariableStore::set` marks
nothing dirty; invalidation lives entirely in the callers, and the store is
handed out via `SurfaceRegistry::variables()`. Nothing in the type system stops
a future call site from writing a value that never repaints anything. The write
path should go through the engine, or `set` should return a token the caller
must dispatch.

## Part 2 — Companion in Rust, Under Our Constraints

A useful exercise, because it separates what Companion got wrong from what its
environment made unavoidable.

**One typed value channel instead of three.** Companion moves data across the
plugin boundary three ways: variables (string-ish), feedback values (JSON since
1.13), and advanced feedback style blobs (up to and including raw pixels with a
declared `pixelFormat`). All three exist because there was no single typed
channel that could carry a colour. A `VariableValue` enum with `Color` and
`Image(AssetId)` variants collapses them into one. Half of Companion's feedback
complexity is a workaround for JSON's type system.

**Host-side expression evaluation from the start.** Companion reached this over
three designs and five years: `parseVariablesInString` called from inside plugin
callbacks, then host-side option pre-parsing in 1.13, then a real expression
language. In-process in Rust the evaluator sits on the render path with no IPC,
and the entire failure mode where a plugin blocks during a render — the reason
`evaluate` carries a "must be cheap and must not perform I/O" warning in
launchpi's own trait today — never arises.

**Stable ids, cosmetic labels.** Companion namespaces variables by the user's
editable connection label, so `$(mydeck:tally_1)` breaks when the user renames
the connection, and `internal:custom_x` is emitted forever as an alias for an
earlier rename. launchpi already has the right shape — `IntegrationId` is
`<type>.<name>` derived from the filename — and should never grow a display name
that participates in references.

**Sharing instead of serialising.** `png64`, `imageBuffer`,
`imageBufferEncoding.pixelFormat` and EJSON `$binary` all exist to move pixels
across a JSON boundary. In-process, an image is an `Arc<[u8]>` and an
`AssetId`, and the entire apparatus disappears.

**A trait instead of a protocol as the extension point.** Companion's stable
surface is a TypeScript class, but the thing actually between host and module is
a wire protocol — so every capability is defined twice, once in
`module-api/` and once in `host-api/api.ts`, and they drift. The warning comment
at the top of `api.ts` is an admission that this is being managed rather than
avoided. In Rust, one trait is both definition and boundary. When
out-of-process becomes necessary it is a second `impl Plugin`, and the trait
remains the single definition — which is precisely what `plugin.rs`'s
*"Nothing here assumes the implementation is in-process"* is preserving.

**Cancellation in the signature, not bolted on.** Companion has a hard 5000 ms
timeout on every call with no way to cancel until v2 added
`direction: 'cancel'` and `AbortSignal`, which they needed because feedback
rechecks were starving one another. `CancelToken` already exists in
`PluginContext`; it should be threaded into `invoke` too, so a long-running
action stops when its instance is reconfigured.

**The API surface is the sandbox.** Companion constructs Node `--permission`
flags from manifest-declared permissions because the module is a foreign
process. An in-process Rust plugin gets exactly what `PluginContext` hands it —
the shared HTTP client, the asset store, the value sink — and nothing else, by
construction. No permission model needs to exist.

**What Rust does not fix.** Compiled-in plugins mean adding an integration
requires rebuilding the daemon. Companion's entire architecture — the process
boundary, the wire protocol, the module store, the upgrade scripts — exists so
that a stranger can ship an integration without touching the core, and that is a
genuine capability launchpi does not have. The honest options are a curated
compiled-in set, WASM components, or an out-of-process protocol, and none of
them is free.

Which is the argument for Part 3: **the fastest route to hundreds of
integrations is not building an ecosystem, it is consuming one.**

## Part 3 — External Companion as a Plugin

### The idea

Point launchpi at a Companion instance on the network. Its connections, their
variables, their buttons and eventually their presets become available inside
launchpi as an ordinary plugin instance, referenced the same way as anything
else:

```toml
[panels.controls.default_state]
text             = "$(companion.studio:atem.program_input)"
background_color = "$(companion.studio:atem.preview_tally)"
```

This is worth doing on its own merits — many people who would run launchpi
already run Companion — but its strategic value is larger. It converts
Companion's entire module ecosystem into launchpi data without launchpi
implementing a single line of Companion's module protocol.

### Why this rather than loading Companion modules directly

The alternative is running real Companion modules inside launchpi. It is
technically possible, and worth stating precisely so the comparison is fair.

A pure-Rust host could speak the v1 protocol: reimplement Node's `'ipc'` stdio
channel (`NODE_CHANNEL_FD=3`, newline-delimited JSON over a socketpair —
undocumented but stable), implement EJSON, implement the ~32-message envelope
and the `register` handshake, persist an upgrade index and drive
`upgradeActionsAndFeedbacks`, pre-substitute variables into option values, and
evaluate JS source text for `isVisibleFn` to render config forms. That reaches
API 1.14.x, a line Bitfocus has already superseded.

The v2 line is closed to that approach — the child's entrypoint is Companion's
own shim and the wire types live in Companion's private tree — but Bitfocus
publishes `@companion-module/host` on npm, described as *"designed to support
multiple versions of the base API and provide a uniform interface back to the
host application."* They built the adapter. A Node sidecar implementing
`ModuleHostContext` against a launchpi protocol would load both v1 and v2
modules.

Either route drags Node, an npm dependency tree, and per-platform Node runtimes
into launchpi's distribution, and commits launchpi to tracking a protocol its
authors explicitly decline to make public. **Consuming a Companion instance gets
most of the same value with none of that**, and the two are not exclusive — the
sidecar remains possible later, as a second `impl Plugin`, if the bridge proves
insufficient.

### Two directions, only one of which is this feature

**launchpi as a Satellite surface of Companion.** launchpi registers over the
Satellite protocol (TCP 16622) and its hardware appears in Companion's Surfaces
table. Companion renders, launchpi displays, presses go back up. This is cheap,
supported, versioned, and gets everything Companion can do immediately.

It is also a different feature. In this mode a key is a *Companion* key:
launchpi's panels, values and rendering are bypassed entirely, and Companion's
page model takes over the surface. It answers "I already run Companion, let me
use my Launchpad with it". It does not let a Companion tally value drive a
launchpi-rendered button.

Worth building — the protocol is well suited to launchpi's hardware, supporting
mixed style presets in one surface so a Launchpad's RGB pads (`colors: 'hex'`),
a Stream Deck's LCD keys (`bitmap: {w,h}`) and a Studio's encoder rings (`leds`,
segment 0 at six o'clock, clockwise — matching the existing Studio dial-ring
geometry) coexist. But it belongs in the surfaces roadmap, not here.

**launchpi as a Companion client.** This is the feature: a `companion` plugin
type, one instance per Companion, publishing that Companion's state as launchpi
values and its buttons as launchpi actions.

### Transport: three protocols, layered

No single Companion protocol provides what is needed. The plugin should use
three, layered by how much it can trust each.

| Capability | HTTP API | Satellite subs | tRPC WS |
| --- | --- | --- | --- |
| Documented and versioned | yes | yes | **no** |
| Enabled by default | yes | **no** | yes |
| Connection list and status | ✓ poll | ✗ | ✓ push |
| Read one named variable | ✓ poll | ✗ | ✓ |
| Enumerate variable names | ✗ | ✗ | ✓ push |
| Action / feedback definitions | ✗ | ✗ | ✓ push |
| Presets | ✗ | ✗ | ✓ push |
| Rendered button image | ✗ | ✓ push, size negotiable | ✓ push, fixed 288 px |
| Press a button | ✓ | ✓ | ✓ |
| Write a custom variable | ✓ | ✓ | ✓ |

The layering follows directly:

- **HTTP is the floor.** `GET /api/connections`, `GET /api/connections/:id/status`,
  `GET /api/variable/:label/:name/value`, `POST /api/location/.../press`,
  `POST /api/custom-variable/:name/value`. Documented, stable, on by default.
  An instance that can reach only this is `Running` with reduced capability.
- **tRPC is enrichment.** A plain-JSON WebSocket at `/trpc` — `initTRPC.create()`
  is called with no transformer, so no JS-specific encoding is involved, and
  there is no authentication provided the client omits the `Origin` header.
  It is the *only* source of variable enumeration, action and feedback
  definitions, and presets. It is also internal, unversioned and undocumented,
  so the plugin must treat every subscription as best-effort and degrade to the
  HTTP floor rather than failing the instance.
- **Satellite subscriptions are optional.** `ADD-SUB` yields `SUB-STATE` on
  every redraw at a resolution launchpi chooses, which is strictly better than
  tRPC's fixed 288 px preview. But it is gated behind
  `satellite_subscriptions_enabled`, which defaults to false, so the UI must be
  able to say "enable subscriptions in Companion to use this".

This maps cleanly onto the `Degraded { reason }` status from Part 1.

### How it maps onto launchpi's model

**Namespacing.** A Companion instance holds many connections. The plugin
publishes `<connection>.<variable>` as its value name, so
`$(companion.studio:atem.program_input)` reads the `program_input` variable of
the connection labelled `atem` on the Companion instance named `studio`.
`plugins.md` already permits exactly this — *"a value name is free-form and may
be structured"*, with `hass.home:light.kitchen.color` as its own example — so no
change to the value model is required.

**Fix Companion's rename bug at the boundary.** Companion namespaces by the
user's editable connection label, so renaming breaks every reference.
`instances.connections.watch` returns `{id, label, moduleId, enabled, status}`,
with `id` stable. The plugin should key on `id` internally and expose `label`
only as a display name in the picker, so a rename inside Companion does not
break a launchpi panel. This is launchpi repairing an inherited flaw rather than
importing it.

**Subscriptions do real work here.** tRPC has no bulk variable-value
subscription: `variables.values.connection` is a query the web UI polls at 1 Hz,
and push requires one `preview.expressionStream.watchExpression` subscription
per variable. Opening one for every variable in a large Companion install is
untenable.

`Plugin::subscribe` solves this exactly. launchpi already tells each instance
which value names anything on screen is watching, as the full current set rather
than a delta. The Companion plugin opens precisely that many expression
subscriptions and closes the rest. A panel binding four Companion values costs
four subscriptions regardless of how large the Companion install is.

This is the mechanism's second proof — it was designed for narrowing a Home
Assistant firehose, and it turns out to be load-bearing for a completely
different upstream. That is a good sign about the abstraction.

**Rendered Companion buttons as image values.**
`preview.graphics.location {pageNumber, row, column}` pushes a data-URL PNG on
every redraw. The plugin can store those through the asset store and publish the
`AssetId` as a value, so a launchpi key can display a Companion button:

```toml
image = "$(companion.studio:button.1.0.2)"
```

This is the escape hatch that makes the bridge complete. Any Companion
capability launchpi has no model for still renders, because launchpi is showing
Companion's own pixels.

**Actions are the weak point, and this needs verifying before it is promised.**
Companion has no external API for "run connection X's action with these
options". The HTTP API presses a *button at a location*; OSC, TCP and Rosstalk
do the same. So the honest first version offers three actions —
`press_location`, `set_custom_variable`, and `set_step` — which requires the
user to have set the action up on a Companion button first.

Publishing Companion's action *definitions* is only useful if they can be
invoked. The tRPC `controls` router has `entities`, `steps` and `styles`
sub-routers that were not exhaustively enumerated during research, and it is
plausible that an entity can be executed directly, or that a scratch control can
be created, populated, pressed and removed. **This must be established by
reading `companion/lib/Controls/` before the definitions-import phase is
planned**, and if no clean path exists, the feature is "press Companion buttons
and read Companion values", which is still worth building.

**Discovery.** Companion advertises `_companion-satellite-tcp._tcp` over mDNS
with a TXT record carrying `id`, `version` and `protocolVersion`. The add-instance
dialog can browse for it and pre-fill the host, and screen `protocolVersion`
before offering the Satellite-backed features.

### Security

The tRPC WebSocket has no authentication and returns, to any client that omits
an `Origin` header, `admin_password`, `prometheus_token`, stored HTTPS private
keys via `userConfig.watchConfig`, and per-connection module secrets via
`instances.connections.watchEdit`.

Three rules follow, and they should be enforced in the plugin rather than left
to discipline:

1. **Never subscribe to `userConfig`.** Nothing the bridge needs is in it.
2. **Never call `watchEdit`.** Use `watch`, which returns no secrets.
3. **Never persist anything read from Companion.** Values are re-derived on
   start, exactly as `values.rs` already reasons about plugin-published values.

The instance's own configuration is just a URL, so there is no secret to store
on launchpi's side either. That is worth preserving: if Companion ever grows
authentication, the token becomes a `ConfigField::secret` and the existing
env/file secret machinery covers it.

It should also be stated plainly in the UI that pointing launchpi at a Companion
means trusting the network between them, because Companion's own documentation
says the same: *"Although none of these features makes an installation secure,
they can help to stop casual browsers."*

### Staging

Phase 0 is a hard prerequisite; the rest are independently shippable.

| Phase | Delivers | Depends on |
| --- | --- | --- |
| **0** | Runtime-pushed definitions (Part 1, change 1) | — |
| **1** | `companion` plugin over HTTP only. Config is a URL. Connection statuses as values; user-declared variables polled by name; `press_location` and `set_custom_variable` actions | 0 |
| **2** | tRPC enrichment. Auto-discovered connections and variable definitions; `expressionStream` subscriptions driven by `Plugin::subscribe`; `Degraded` when unavailable | 1, plus `Degraded` status |
| **3** | Button images via `preview.graphics.location`, published as `AssetId` values | 2, plus the asset store and `KeyRendering.image` |
| **4** | Presets imported from `instances.definitions.presets` | 2, plus presets (Part 1, change 2) |

Separately and with no dependency on any of the above: **Satellite surface
mode**, launchpi presenting its hardware to Companion. Different direction,
different feature, belongs in the surfaces roadmap.

### Risks

- **tRPC is unversioned and undocumented.** The procedures used are unchanged
  between 5.0.2 and 5.1.0-dev, which is evidence and not a promise. Every
  subscription must fail soft into the HTTP floor, and the plugin should record
  the Companion version it saw in `BEGIN` or `appInfo` so a mismatch is
  diagnosable from the instance log.
- **Action invocation may have no clean path.** Establish this before promising
  definition import; see above.
- **Satellite subscriptions default to off**, and toggling
  `satellite_subscriptions_enabled` force-closes every satellite socket on that
  Companion — including launchpi's own, if it is also connected as a surface.
- **A Companion connection rename** breaks references unless the plugin keys on
  connection id, as described.
- **No authentication anywhere** means the bridge is only as safe as the
  network. This is Companion's posture, not a launchpi choice, and it should be
  documented rather than mitigated.

## References

- `companion-research.md` — the source material for every claim about Companion
- `plugins.md` — the current design of record
- `plugin-authoring.md` — the practical guide, which still describes feedbacks
- `configuration.md` — file schemas and secret handling
