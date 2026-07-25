# Configuration

## Scope

This document is the schema reference for every file launchpi reads, and the
contract behind the copy-TOML buttons in the web UI.

The goal stated in `plan.md` is that configuration made in the web UI can be
saved as a config file, and that the config file can be declarative. This is
what makes that true: what the daemon writes, what the UI copies, and what a
Nix generator would produce are the same documents.

## Locations

| Path | Contents |
| --- | --- |
| `$LAUNCHPI_CONFIG_DIR`, else `$XDG_CONFIG_HOME/launchpi`, else `~/.config/launchpi` | configuration |
| `$LAUNCHPI_STATE_DIR`, else `$XDG_STATE_HOME/launchpi`, else `~/.local/state/launchpi` | volatile runtime state |
| `$XDG_CACHE_HOME/launchpi`, else `~/.cache/launchpi` | decoded and fetched assets |

```text
~/.config/launchpi/
  devices.toml
  panels.toml
  values.toml
  plugins/
    http.weather.toml
    mpris.default.toml
    hass.home.toml
```

State and cache are both reconstructible. `runtime.sqlite3` holds only the last
known connection status per surface, and the asset cache is content-addressed.
Deleting either costs nothing but a reconnect and a re-fetch. Configuration is
the only directory worth backing up or checking in.

Writes are atomic: the daemon writes `<name>.toml.tmp` and renames.

## devices.toml

Physical hardware endpoints. Each device declares its layout and capabilities
and may select one compatible active panel.

```toml
version = 1

[[devices]]
surface_id = "stream-deck-studio-2"
name = "Stream Deck Studio 025912._elg._tcp.local."
host = "10.0.0.195"
port = 5343
serial_number = "A9IJA541301ZUS"
model = "Stream Deck Studio"
active_panel_id = "studio-panel-1"
is_enabled = true

[devices.layout.Grid]
columns = 16
rows = 2

[devices.capabilities]
supports_color = true
supports_images = true
supports_text = true
supports_brightness = true
supports_haptics = false
```

| Field | Type | Notes |
| --- | --- | --- |
| `surface_id` | string | Stable identity; referenced by nothing else, but changing it orphans the device's runtime status row. |
| `name` | string | Display name. Discovery seeds it from the mDNS service name. |
| `host`, `port` | string, integer | TCP endpoint. Port defaults to `5343`. |
| `serial_number` | string, optional | Used to deduplicate a discovered device against a configured one. |
| `model` | string | Matched against known model names to pick layout and quirks, such as the Stream Deck XL image flip. |
| `layout` | enum | `[devices.layout.Grid]` with `columns` and `rows`, or `layout = "Freeform"` for keyless surfaces such as the Network Dock. |
| `capabilities` | table | Gates which panels may be assigned. |
| `active_panel_id` | string, optional | Must name a panel whose layout and capabilities are compatible. |
| `is_enabled` | boolean | A disabled device is not connected to. |

Connection status and last error are runtime state and never appear here.
Network Dock children are enumerated from the dock at connect time and are not
persisted either; adding one by hand has no effect.

## panels.toml

Reusable virtual control grids. A panel is not bound to a device; a device
selects a panel.

```toml
version = 2

[[panels]]
panel_id = "studio-panel-2"
name = "Auto 8x4"
dial_ring_levels = [24, 67]

[panels.layout]
columns = 8
rows = 4

[panels.capabilities]
supports_color = true
supports_images = true
supports_text = true
supports_brightness = true
supports_haptics = false

[[panels.controls]]
control_id = "auto-0-0"
name = "Play"

[panels.controls.position]
column = 0
row = 0

[panels.controls.default_state]
text = "$(mpris.default:title)"
image = "$(mpris.default:art)"
is_pressed = false

[panels.controls.default_state.foreground_color]
red = 255
green = 255
blue = 255
alpha = 255
```

`version` is `2` as of the plugin system. The loader accepts `1` and `2`, and
always writes `2`.

### Control

| Field | Type | Notes |
| --- | --- | --- |
| `control_id` | string | Unique within the panel. |
| `name` | string | Editor label only; never rendered. |
| `position` | table | `column` and `row`, both zero-based. Two controls may not share a cell, and a control outside the layout is a validation error. |
| `default_state` | table | See below. |
| `pressed_state` | table, optional | Falls back to `default_state` when absent. |
| `action_bindings` | array | What the control does. |

The key index sent to hardware is `row * columns + column`.

### Rendered state

| Field | Type | Notes |
| --- | --- | --- |
| `text` | string, optional | Interpolated: `$(instance:variable)`, with `$$` for a literal dollar. |
| `image` | string, optional | An `AssetId`, or a `$(...)` reference that resolves to one. |
| `overlay_image` | table, optional | A badge drawn small in a corner and never dimmed by the label scrim. `image` is an `AssetId` or a `$(...)` reference; `anchor` is one of the nine positions, default `bottom_end`; `scale_percent` is its size as a share of the key, default `32`. |
| `foreground_color`, `background_color` | table or string, optional | A table of `red`, `green`, `blue`, `alpha` is a fixed colour. A `"$(...)"` string binds the colour to a value, so a key can take a light's real colour. |
| `border` | table, optional | `color` takes the same forms as the colours above; `width` is the inset frame's depth in pixels, default `5`. A `color` that does not resolve draws no frame. |
| `progress` | table, optional | `value` and `maximum_value`; drawn as a bar along the bottom edge. |
| `is_pressed` | boolean | Present on `default_state` and `pressed_state` for symmetry; the daemon does not read it. |

`AssetId` takes one of three forms:

```text
builtin:<shape>     circle, diamond, pause, play, square, triangle
file:<path>         a path on disk
hash:<sha256>       an entry in the asset cache, published by a plugin
```

### Action bindings

```toml
[[panels.controls.action_bindings]]
gesture = "press"

[[panels.controls.action_bindings.actions]]
type = "invoke_integration"
integration_id = "hass.home"
action_name = "light.toggle"
parameters = { entity_id = "light.kitchen" }

[[panels.controls.action_bindings.actions]]
type = "wait"
duration_ms = 200

[[panels.controls.action_bindings.actions]]
type = "change_panel"
panel_id = "studio-panel-1"
```

`gesture` is one of `press`, `release`, `hold`, `rotate_clockwise`,
`rotate_counter_clockwise`, `value_changed`. `hold` carries a duration:

```toml
gesture = { hold = { duration_ms = 800 } }
```

Actions within a binding run in order, and `wait` genuinely pauses the chain.
A failing action logs and the chain continues.

| Action `type` | Fields |
| --- | --- |
| `invoke_integration` | `integration_id`, `action_name`, `parameters` |
| `set_variable` | `variable_name`, `value`. Unqualified names land in the `user` namespace, readable as `$(user:name)`. |
| `change_panel` | `panel_id` |
| `wait` | `duration_ms` |

### Binding a field to a value

Any of `text`, `image`, `overlay_image.image`, `foreground_color`,
`background_color` and `border.color` may hold a `$(instance:value)` reference
instead of a literal. The control repaints
whenever the referenced value changes.

```toml
[panels.controls.default_state]
text             = "$(mpris.default:title)"
background_color = "$(hass.home:light.kitchen.color)"

[panels.controls.default_state.foreground_color]
red = 255
green = 255
blue = 255
alpha = 255
```

A colour reference resolves through the value's rendered text, so a plugin may
publish either a structured colour or a `#rrggbb` string. `#rgb` and `#rrggbbaa`
are accepted too. A reference that resolves to something unparseable leaves the
colour unset rather than painting the key black, so a plugin that has not
answered yet looks unstyled rather than broken.

There is no separate feedback concept. An earlier draft had plugins declare
boolean queries that overlaid a style; binding the field directly does the same
job and can also carry a colour, which a boolean never could.

## values.toml

Values you define yourself, as opposed to ones a plugin publishes. They live in
the `user` namespace, are seeded at boot, and are what `Action::SetVariable`
writes to.

```toml
version = 1

[[values]]
name = "scene"
value = "evening"
description = "Which lighting scene the panels assume"
```

`value` may be a string, a number or a boolean; its TOML type decides the value
kind. Read it back as `$(user:scene)`.

These are the only values worth persisting — everything else is re-derived from
its source when an instance starts.

## plugins/

One file per plugin instance. The filename is the identity: `<type>.<name>.toml`
becomes the `integration_id` `<type>.<name>`, where `name` matches
`[a-z0-9][a-z0-9-]*`. There is no index file — the directory listing is the
instance list, so adding an instance declaratively is adding a file.

A file whose `<type>` is not compiled into the daemon loads as an instance in
the error state and is shown in the UI with the reason. It does not stop the
daemon from starting.

```toml
# plugins/http.weather.toml
version = 1
enabled = true
display_name = "Weather"

[config]
base_url = "https://api.open-meteo.com"
timeout_ms = 5000
authorization = { env = "LAUNCHPI_WEATHER_KEY" }

[[config.poll]]
name = "temperature"
path = "/v1/forecast?latitude=52.37&longitude=4.89&current=temperature_2m"
interval_ms = 60000
extract = "current.temperature_2m"
```

| Field | Type | Notes |
| --- | --- | --- |
| `version` | integer | Instance file schema version, currently `1`. |
| `enabled` | boolean | A disabled instance is not started. Its bindings resolve to nothing rather than to an error. |
| `display_name` | string, optional | UI label. Defaults to the instance id. |
| `config` | table | Shape is defined by the plugin's manifest and validated at start. |

The keys under `[config]` are plugin-specific; each plugin's manifest declares
them, which is also what generates its form in the web UI. See the per-plugin
sections in `plugin-authoring.md`.

## Secrets

Any configuration field a plugin declares as a secret accepts three forms:

```toml
token = { env = "LAUNCHPI_HASS_TOKEN" }   # read from the environment at start
token = { file = "/run/agenix/hass-token" }  # read from disk at start, trimmed
token = "eyJhbGciOi..."                    # inline
```

Resolution happens once, when the instance starts. A missing variable or an
unreadable file puts that instance into the error state with a readable message;
it never panics and never starts the plugin with an empty credential.

Instance files are written with mode `0600` because the inline form is
permitted. Prefer `env` or `file` on any machine where the config directory is
not private.

**Exports never contain an inline secret.** Every copy-TOML and config export
path replaces an inline value with a reference placeholder:

```toml
token = { env = "LAUNCHPI_HASS_HOME_TOKEN" }
```

The placeholder name is derived from the instance id and the field name, so the
exported document is not merely redacted — it is directly usable, given the
environment variable. That is the property that makes a copy-TOML button safe to
press on a machine whose config is about to be pasted into a repository.

The web UI edits which form a secret reference takes. It never displays a stored
inline value back to the browser.

## Copy TOML and declarative use

Every export endpoint emits a document in the same schema the daemon loads, not
a separate export format. A copied panel is a one-panel `panels.toml`; a copied
device is a one-device `devices.toml`; a copied plugin instance is that
instance's file.

| Source | Endpoint | Emits |
| --- | --- | --- |
| Panel editor | `GET /api/panels/:panel_id/config` | one-panel `panels.toml` |
| Device detail | `GET /api/devices/:surface_id/config` | one-device `devices.toml` |
| Plugin instance | `GET /api/plugins/:integration_id/config` | one instance file |
| Whole config | `GET /api/config/export` | every file, separated by path comments |

Two consequences worth stating. Concatenating two copied panels under a single
`version` header is a valid `panels.toml`, because the schema is a list.
And round-tripping is exact apart from secrets: pasting an export into a fresh
`LAUNCHPI_CONFIG_DIR` reproduces the same state.

For a declarative setup, generate these files into the config directory and set
`LAUNCHPI_CONFIG_DIR` to point at them. Runtime state and the asset cache stay
outside it and need no special handling. Note that the daemon writes to the
config directory whenever the UI mutates something, so a read-only or
store-managed config directory makes the UI's save paths fail — that is the
intended trade for determinism, but it should be a deliberate choice rather than
a surprise.

## References

- `plugins.md` for the plugin system design
- `plugin-authoring.md` for adding a plugin type
- [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/latest/)
