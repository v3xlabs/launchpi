# Key layers

Nothing here is implemented. This is the design for replacing the flat `RenderedState` with an
ordered list of layers, and for icons, which fall out of it almost for free.

## Why

`RenderedState` has grown a field per idea:

```rust
pub struct RenderedState {
    pub text: Option<String>,
    pub image: Option<AssetId>,
    pub overlay_image: Option<OverlayImage>,
    pub foreground_color: Option<ColorBinding>,
    pub background_color: Option<ColorBinding>,
    pub border: Option<Border>,
    pub progress: Option<Progress>,
    pub content_layout: ContentLayout,
    pub is_pressed: bool,
}
```

Three problems, and they are the same problem.

**Fields that do the same job are different fields.** `background_color` and `image` both cover the
key; one is a colour and one is a picture, and nothing else distinguishes them. `image` and
`overlay_image` are both pictures, differing only in that one fills and one sits in a corner.
`foreground_color` belongs to the text but is stored beside it rather than on it.

**The draw order is hardcoded in the rasteriser, not in the data.** `render_key_image`
(`drivers/streamdeck/studio.rs`) fixes it: fill, art, icon, text, progress, overlay, border. A badge
under the label, or a border beneath a picture, is unrepresentable — not because anyone decided
against it, but because the order lives in a function.

**Every new idea costs a field everywhere.** `border` and `overlay_image` each had to be added to
`RenderedState`, `ResolvedState`, `resolve_states`, `references_of_state`, `KeyRendering`, two
`KeyRendering` construction sites, the rasteriser, the web types, the web guard and the inspector.
Ten places for one visual idea. The next idea costs the same again.

## The model

A state is a stack. Layers draw in array order, index 0 first, so later entries sit on top.

```rust
pub struct RenderedState {
    #[serde(default)]
    pub layers: Vec<Layer>,
    pub is_pressed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Layer {
    Fill {
        color: ColorBinding,
    },
    Image {
        image: AssetId,
        #[serde(default)]
        fit: Fit,
        #[serde(default)]
        anchor: Anchor9,
        #[serde(default = "full_scale")]
        scale_percent: u8,
        /// Recolours a monochrome source. Absent leaves the picture as it is.
        tint: Option<ColorBinding>,
    },
    Text {
        text: String,
        color: ColorBinding,
        #[serde(default)]
        anchor: Anchor9,
    },
    Bar {
        value: ValueBinding,
        maximum: ValueBinding,
        color: ColorBinding,
        #[serde(default)]
        edge: Edge,
        thickness: u8,
    },
    Border {
        color: ColorBinding,
        width: u8,
    },
}

/// `Cover` crops to fill the key; `Contain` fits the whole picture inside it.
pub enum Fit { Cover, Contain }
```

`Anchor9` and `ColorBinding` are unchanged. Every visual field becomes bindable by construction,
which quietly fixes `progress`: it is a literal today and cannot follow a value at all.

### Icons are an asset scheme, not a layer

`AssetId` is already an open vocabulary resolved by `AssetStore`: `hash:`, `http(s)://`, `file://`.
An icon is one more spelling.

```
mdi:lightbulb-on      Material Design Icons
si:discord            Simple Icons
```

`AssetStore` gains an SVG path and rasterises at whatever size the layer asks for. Nothing in the
layer model knows what an icon is — an icon is a picture, and `tint` on the `Image` layer is what
makes a lightbulb take a light's colour.

This also settles `builtin:play`, which `docs/configuration.md` has documented and the code has
never implemented: either it becomes a real bundled icon set or it goes.

**Cost:** `image` is built with `jpeg`, `png`, `webp` and `gif` and cannot decode SVG. Rasterising
needs `resvg` — pure Rust, no system libraries. With `--no-default-features` it drops `text`,
`system-fonts` and `raster-images`, which icon glyphs (pure paths) do not need. That is the whole
dependency ask; it needs approval before anyone writes the code.

## What folds into what

| Today | Becomes |
| --- | --- |
| `background_color` | `Fill` |
| `image` | `Image { fit: Cover }` |
| `overlay_image` | `Image { fit: Contain, anchor, scale_percent }` |
| `text` + `foreground_color` + `content_layout.text_anchor` | `Text` |
| `progress` | `Bar` |
| `border` | `Border` |
| the art scrim | a translucent `Fill` between the picture and the text |

The scrim is the one thing that gets worse. Today `render_key_image` darkens the art whenever there
is text, automatically. As a layer it becomes something a preset author has to place deliberately.
That is more honest — the darkening is a real thing on the key and today it is invisible in the
data — but it is a papercut, and the v4 migration must insert it so nothing changes appearance.

`Control.name` stays what it is: the key's name in the editor and in `panels.toml`, never drawn.
Drawn text is a `Text` layer. They read similarly and are genuinely different things, and collapsing
them would leave a control with no name when its stack has no text.

`pressed_state` stays a whole alternate stack, falling back to the default stack as it does now.
Per-layer "only when pressed" is the obvious alternative and is worse: it puts a condition on every
layer to express something two stacks already say.

## What it costs

- **`PANEL_DOCUMENT_VERSION` 4 → 5**, immediately after the dial bump. The migration is mechanical:
  each old state becomes the layer list above, in today's draw order, scrim included. It must be
  exhaustive — a field missed in the migration is a key that silently loses part of its face.
- **`references_of_state`** (`rendering/index.rs`) walks every bindable field of every layer. Its
  own doc comment already warns that a missed field is "invisible twice over": the plugin is never
  told to watch the value and the key never repaints. A layer list makes this both easier to get
  right (one loop) and worse to get wrong (more fields).
- **The rasteriser** becomes a fold over layers instead of a fixed sequence. Each layer type is a
  small function; `draw_border` and `draw_overlay` already have the right shape.
- **`AssetStore`** can drop the split between `decoded` (RGB, cover) and `decoded_rgba` (RGBA, fit),
  which exists only because the two current image fields want different things. Layers composite
  with alpha throughout, so one RGBA path serves both.
- **Launchpads** take a single palette colour and have no concept of a stack. The rule should be the
  bottom-most `Fill`, else black, written down rather than inferred.
- **The inspector** becomes a layer list — add, remove, reorder, and a small editor per layer type.
  This is the largest piece of work and the one that decides whether the model feels better or just
  more general. A new control must default to `[Fill, Text]` so the common case still takes two
  fields, not two layers plus ceremony.
- **Every preset** is rewritten, in Discord and Home Assistant both.

## Open questions

- Does a layer need a name, so the inspector can list "Background / Avatar / Label" rather than
  "Fill / Image / Text"? Cheap to add, easy to leave out, annoying to retrofit.
- Does `Bar` deserve to exist, or is a progress bar a `Fill` with a bound width once layers can
  express that? Keeping `Bar` is the smaller step; folding it in is the more honest one.
- Which icon pack ships. MDI alone is ~7400 glyphs; the picker needs search either way.

## Sequencing

1. Layers in the daemon: the model, the v5 migration, the render path, the dependency index. No web
   changes yet — the migration keeps every existing panel rendering exactly as it does now.
2. The inspector: layer list, per-layer editors, sensible defaults.
3. SVG in `AssetStore` and the `mdi:` scheme, once the dependency is approved.
4. The icon picker in the web.
5. Rewrite the Discord and Home Assistant presets onto layers, and give Home Assistant lights a
   tinted lightbulb — the thing that prompted all of this.
