# Glossary

One name per concept, and one meaning per name. Where two words could describe the same thing, this
file picks one and the rest of the codebase follows it. A synonym found in the code or the docs is a
mistake to be fixed, not an alternative.

Each entry says what the thing is, and how it is written in TOML.

## Where configuration lives

`$LAUNCHPI_CONFIG_DIR`, else `$XDG_CONFIG_HOME/launchpi`, else `~/.config/launchpi`.

| File | Holds | Version |
| --- | --- | --- |
| `devices.toml` | every **device** the daemon manages | 1 |
| `panels.toml` | every **panel**, its **controls** and its **dials** | 5 |
| `values.toml` | **user values** | 1 |
| `plugins/<type>.<name>.toml` | one **plugin instance** | 1 |

Assets are not configuration. They are cached under `$LAUNCHPI_CACHE_DIR`, else
`~/.cache/launchpi/assets`, and can be deleted at any time.

---

## Hardware

### Surface
Anything the daemon can draw on and take input from. **Surface** is the general word; **device** is
the specific one. Prefer *device* when talking about real hardware.

### Device
One piece of hardware, identified by a `surface_id`. Written in `devices.toml`:

```toml
version = 1

[[devices]]
surface_id = "stream-deck-studio-1"
name = "Studio"
host = "10.0.0.195"
port = 5343
model = "Stream Deck Studio"
active_panel_id = "studio-panel-1"
is_enabled = true
```

`surface_id` is opaque. It begins `stream-deck-studio-` for every Stream Deck whatever the model,
for historical reasons — never read a model out of it.

### Model
What kind of hardware a device is, identified by its USB product id. The model table
(`drivers/streamdeck/model.rs`) is the **only** place a model's key grid and dial geometry are
written down. Not configuration: it is compiled in, because it describes hardware rather than
choices.

### Layout
The key grid a surface has: `Grid { columns, rows }`, or `Freeform` for a hub with no keys of its
own. A device's layout comes from its model; a panel's layout is declared.

### Capabilities
What a surface can do: `supports_color`, `supports_images`, `supports_text`, `supports_brightness`,
`supports_haptics`. A panel declares the capabilities it *requires*, and can only run on a device
that has all of them.

### Dial
A rotary knob with a lit ring. Two separate things share the word and must not be confused:

- **Dial placement** — where a knob physically sits. Comes from the model, never configured.
- **Panel dial** — what a panel paints on that knob. Configured.

A dial the active panel does not declare is dark and does not respond to being turned.

### Ring, segment, level
The lit circle around a knob is the **ring**. It is divided into 24 **segments**, one per detent of
the knob. **Level** is how much of the ring is lit, as a percentage.

---

## Panels and keys

### Panel
A reusable arrangement of keys and dials. A panel is not tied to one device: any device with a
matching layout and the required capabilities can run it.

```toml
version = 5

[[panels]]
panel_id = "studio-panel-1"
name = "Hello"

[panels.layout]
columns = 16
rows = 2

[panels.capabilities]
supports_color = true
supports_images = true
supports_text = true
supports_brightness = true
supports_haptics = false

[[panels.dials]]
index = 0
level = 90

[panels.dials.color]
red = 174
green = 255
blue = 0
alpha = 255
```

### Control
One key on a panel. **Control** is the word in the code and in TOML; **key** is the word for the
physical button. A control has a `name` (how you refer to it in the editor — never drawn), a
`position`, a **default state**, an optional **pressed state**, and its **action bindings**.

```toml
[[panels.controls]]
control_id = "hello"
name = "Hello"

[panels.controls.position]
column = 0
row = 0
```

### State
What a control looks like. A control has a **default state** and optionally a **pressed state**; the
pressed state falls back to the default state where it is absent. A state is a **stack**.

### Stack
The ordered list of **layers** that make up a state. Index 0 is drawn first, so later entries sit on
top. *Stack* is the idea; `layers` is the field.

```toml
[panels.controls.default_state]
is_pressed = false

[[panels.controls.default_state.layers]]
kind = "fill"
color = { red = 30, green = 41, blue = 59, alpha = 255 }

[[panels.controls.default_state.layers]]
kind = "text"
text = "$(mpris.default:title)"
color = { red = 255, green = 255, blue = 255, alpha = 255 }
anchor = "bottom_center"
```

`is_pressed` is present on both states for symmetry, and the daemon does not read it.

### Layer
One thing drawn on a key. Five kinds, each named by `kind`:

| `kind` | Draws | Fields |
| --- | --- | --- |
| `fill` | a colour over the whole key | `color` |
| `image` | a picture | `image`, `fit`, `anchor`, `scale_percent`, `tint` |
| `text` | a label | `text`, `color`, `anchor` |
| `bar` | a bar along one edge | `value`, `maximum`, `color`, `edge`, `thickness` |
| `border` | an inset frame | `color`, `width` |

A layer that resolves to nothing drawable is dropped, so a plugin that has not answered yet leaves
the key as if the layer were not there.

### Scrim
A `fill` layer with an alpha below 255, placed over a picture to keep a label readable. Not a
separate concept — it is what a translucent fill is *for*.

### Fit
How an `image` layer uses its square. `cover` crops the picture to fill it; `contain` fits the whole
picture inside it.

### Anchor
Where something sits, as one of nine positions: `top_start`, `top_center`, `top_end`,
`center_start`, `center`, `center_end`, `bottom_start`, `bottom_center`, `bottom_end`. *Start* and
*end* rather than left and right, so the words survive a right-to-left layout.

### Edge
Which side a `bar` runs along: `top`, `bottom`, `start`, `end`. Horizontal bars grow from the start
edge, vertical ones from the bottom.

### Tint
A colour multiplied through an `image` layer. A white-on-transparent glyph becomes a coloured one; a
photograph is darkened toward the tint. Absent leaves the picture alone.

### Subpanel
A panel opened on top of another, covering part of the grid. A key it covers is **dimmed**: drawn
black and inert.

---

## Values and bindings

### Value
One named reading published by a plugin instance. Runtime state, never configuration. A value is
`Text`, `Number`, `Boolean`, `Image` or `Color`.

### Reference
How a control names a value: `$(instance:name)`. `$$` is a literal dollar. A reference that resolves
to nothing leaves its field unset rather than defaulting, so an unanswered plugin looks unstyled
rather than wrong.

### Binding
A field holding either a literal or a reference. Two kinds, told apart by how they are written:

- **Colour binding** — a table is a literal, a string is a reference.
  ```toml
  color = { red = 30, green = 41, blue = 59, alpha = 255 }   # literal
  color = "$(hass.home:light.kitchen.color)"                 # reference
  ```
- **Value binding** — an integer is a literal, a string is a reference.
  ```toml
  value = 50
  value = "$(mpris.default:progress)"
  ```

### User value
A value you set yourself rather than one a plugin publishes. Lives in the `user` namespace, so it is
referenced as `$(user:name)`.

```toml
version = 1

[[values]]
name = "scene"
value = "night"
description = "Which scene the panel is showing"
```

### Asset
A picture a key can draw, named by an **asset id**. Every spelling is resolved by the asset store:

| Spelling | Means |
| --- | --- |
| `hash:<sha256>` | bytes a plugin stored |
| `https://…` | fetched once, then cached |
| `file:///…` | read from disk |
| `mdi:<name>` | a Material Design Icon, drawn at the size the layer asks for |
| anything else | nothing is drawn |

An icon is not a separate concept from an image. It is an asset id that happens to name a glyph,
which is why there is no icon layer.

---

## Plugins

### Plugin type
A kind of integration compiled into the daemon: `http`, `mpris`, `hass`, `discord`. Not
configuration.

### Plugin instance
One configured connection of a plugin type, identified by an **integration id** of the form
`type.name`. The filename *is* the identity: `plugins/discord.default.toml` is `discord.default`.

```toml
version = 1
enabled = true

[config]
token = { env = "LAUNCHPI_DISCORD_TOKEN" }
guild_id = "123456789012345678"
```

Prefer **instance** over "connection" or "integration" for one of these.

### Manifest
What a plugin type declares about itself: its configuration fields, its actions, and the values it
can publish. Compiled in, never configured.

### Action
Something an instance can be asked to do. Distinct from a **gesture**, which is what you did to the
key.

### Action binding
A gesture and the actions it runs.

```toml
[[panels.controls.action_bindings]]
gesture = "press"

[[panels.controls.action_bindings.actions]]
type = "invoke_integration"
integration_id = "hass.home"
action_name = "light.toggle"
parameters = { entity_id = "light.kitchen" }
```

### Gesture
What was done to a control: `press`, `release`, `hold` (with `duration_ms`), `rotate_clockwise`,
`rotate_counter_clockwise`, `value_changed`. Only press, release and hold currently fire for keys.

### Subscription
What the daemon tells an instance is on screen, so a plugin can poll only what something is actually
showing. Derived, never configured.

### Lookup
The choices an instance offers for one of its configuration fields — its guilds, its entities, its
voice channels. Answered from what the instance already knows, never from the network.

### Preset
A ready-made control an instance recommends, offered in the editor. Runtime data, never
configuration: what a Home Assistant installation should offer is its own lights, and a plugin
*type* cannot know those.

### Control template
A control without its placement: everything about a button and nothing about where it sits. What a
preset carries. Serialised, it is a `[[panels.controls]]` table with `control_id` and `position`
removed, which is what makes a preset something you can paste into a panel file by hand.

### `self`
What a preset writes instead of its own integration id. The daemon rewrites it to the publishing
instance before anything else sees it, so `$(self:channel_members_0)` reaches the editor already
saying `$(discord.default:channel_members_0)`.

---

## Words this project does not use

| Not this | Use | Because |
| --- | --- | --- |
| button | **control**, or **key** for the physical one | two words for one thing |
| variable | **value** | one word for a published reading |
| feedback | **binding** | there are no boolean feedbacks; a field reads a value directly |
| icon (as a layer) | **image** layer with an `mdi:` asset | an icon is a picture, not a kind of layer |
| overlay, badge | **image** layer, `contain` fit, with an anchor | it was a separate field once; it is not now |
| background, foreground | **fill** layer, **text** layer colour | the stack already says which is which |
| connection, integration | **instance** | one word for one configured plugin |
| knob | **dial** | one word |
