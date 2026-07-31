use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    config::write_toml,
    identifiers::{AssetId, ControlId, PanelId},
    panels::{
        dial::{full_ring, PanelDial},
        rendered_state::{Anchor9, ColorBinding, Edge, Fit, Layer, RgbaColor},
        Panel,
    },
};

#[derive(Deserialize, Serialize)]
struct PanelsDocument {
    version: u8,
    panels: Vec<Panel>,
}

/// Up to version 3 a panel carried two parallel arrays indexed by dial number instead of the
/// dials it declares. Serde ignores the fields it no longer knows, so those files are read a
/// second time through this view and the dials rebuilt from it.
#[derive(Deserialize)]
struct LegacyDialsDocument {
    panels: Vec<LegacyPanelDials>,
}

#[derive(Deserialize)]
struct LegacyPanelDials {
    panel_id: PanelId,
    #[serde(default)]
    dial_colors: Vec<RgbaColor>,
    #[serde(default)]
    dial_ring_levels: Vec<u8>,
}

/// Version 2 tags `Action` and `ActionTrigger` for readable TOML. Version 1 files parse
/// identically because no `action_bindings` written under it were ever non-empty. Version 4
/// replaces the parallel dial arrays with `[[panels.dials]]`. Version 5 replaces a state's fixed
/// fields with `[[layers]]`.
const PANEL_DOCUMENT_VERSION: u8 = 5;
const SUPPORTED_PANEL_VERSIONS: [u8; 5] = [1, 2, 3, 4, PANEL_DOCUMENT_VERSION];
const DIALS_VERSION: u8 = 4;
const LAYERS_VERSION: u8 = 5;

/// Up to version 4 a state was a fixed set of fields drawn in an order the rasteriser chose. Read
/// through this view, each becomes the stack that draws the same picture.
#[derive(Deserialize)]
struct LegacyLayersDocument {
    panels: Vec<LegacyPanelStates>,
}

#[derive(Deserialize)]
struct LegacyPanelStates {
    panel_id: PanelId,
    #[serde(default)]
    controls: Vec<LegacyControlStates>,
}

#[derive(Deserialize)]
struct LegacyControlStates {
    control_id: ControlId,
    #[serde(default)]
    default_state: LegacyState,
    #[serde(default)]
    pressed_state: Option<LegacyState>,
}

#[derive(Default, Deserialize)]
struct LegacyState {
    text: Option<String>,
    image: Option<AssetId>,
    overlay_image: Option<LegacyOverlay>,
    foreground_color: Option<ColorBinding>,
    background_color: Option<ColorBinding>,
    border: Option<LegacyBorder>,
    progress: Option<LegacyProgress>,
    #[serde(default)]
    content_layout: LegacyContentLayout,
}

#[derive(Deserialize)]
struct LegacyOverlay {
    image: AssetId,
    #[serde(default)]
    anchor: Anchor9,
    scale_percent: u8,
}

#[derive(Deserialize)]
struct LegacyBorder {
    color: ColorBinding,
    width: u8,
}

#[derive(Deserialize)]
struct LegacyProgress {
    value: u16,
    maximum_value: u16,
}

#[derive(Default, Deserialize)]
struct LegacyContentLayout {
    #[serde(default)]
    text_anchor: Anchor9,
}

impl LegacyState {
    /// The order the rasteriser used to draw these in, so a migrated key looks like it did.
    fn into_layers(self) -> Vec<Layer> {
        let mut layers = Vec::new();
        let content_color = || {
            self.foreground_color
                .clone()
                .unwrap_or_else(|| RgbaColor::opaque(255, 255, 255).into())
        };

        if let Some(color) = self.background_color.clone() {
            layers.push(Layer::Fill { color });
        }
        if let Some(image) = self.image {
            layers.push(Layer::Image {
                image,
                fit: Fit::Cover,
                anchor: Anchor9::Center,
                scale_percent: 100,
                tint: None,
            });
            // Art was darkened whenever there was a label over it. As a layer that darkening has to
            // be something you can see in the file.
            if self.text.is_some() {
                layers.push(Layer::Fill {
                    color: RgbaColor {
                        red: 0,
                        green: 0,
                        blue: 0,
                        alpha: ART_SCRIM_ALPHA,
                    }
                    .into(),
                });
            }
        }
        if let Some(text) = self.text {
            layers.push(Layer::Text {
                text,
                color: content_color(),
                anchor: self.content_layout.text_anchor,
                font_family: None,
                font_size: None,
            });
        }
        if let Some(progress) = self.progress {
            layers.push(Layer::Bar {
                value: progress.value.into(),
                maximum: progress.maximum_value.into(),
                color: content_color(),
                edge: Edge::Bottom,
                thickness: 6,
            });
        }
        if let Some(overlay) = self.overlay_image {
            layers.push(Layer::Image {
                image: overlay.image,
                fit: Fit::Contain,
                anchor: overlay.anchor,
                scale_percent: overlay.scale_percent,
                tint: None,
            });
        }
        if let Some(border) = self.border {
            layers.push(Layer::Border {
                color: border.color,
                width: border.width,
            });
        }
        layers
    }
}

/// Black at this alpha leaves art at the 45% the rasteriser used to multiply it by.
const ART_SCRIM_ALPHA: u8 = 140;

pub fn load(path: &Path) -> Result<Vec<Panel>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents =
        fs::read_to_string(path).with_context(|| format!("unable to read {}", path.display()))?;
    parse(&contents).with_context(|| format!("unable to parse {}", path.display()))
}

fn parse(contents: &str) -> Result<Vec<Panel>> {
    let config: PanelsDocument = toml::from_str(contents)?;
    if !SUPPORTED_PANEL_VERSIONS.contains(&config.version) {
        anyhow::bail!("unsupported panel configuration version {}", config.version);
    }
    let mut panels = config.panels;
    if config.version < DIALS_VERSION {
        let legacy: LegacyDialsDocument = toml::from_str(contents)?;
        adopt_legacy_dials(&mut panels, legacy.panels);
    }
    if config.version < LAYERS_VERSION {
        let legacy: LegacyLayersDocument = toml::from_str(contents)?;
        adopt_legacy_layers(&mut panels, legacy.panels);
    }
    Ok(panels)
}

fn adopt_legacy_dials(panels: &mut [Panel], legacy: Vec<LegacyPanelDials>) {
    for entry in legacy {
        let Some(panel) = panels
            .iter_mut()
            .find(|panel| panel.panel_id == entry.panel_id)
            .filter(|panel| panel.dials.is_empty())
        else {
            continue;
        };
        let levels = entry.dial_ring_levels;
        panel.dials = entry
            .dial_colors
            .into_iter()
            .enumerate()
            .filter_map(|(index, color)| {
                Some(PanelDial {
                    index: u8::try_from(index).ok()?,
                    level: levels.get(index).copied().unwrap_or_else(full_ring),
                    color,
                })
            })
            .collect();
    }
}

/// Only a state that arrived with no stack is rebuilt, so a file that already speaks version 5 is
/// never second-guessed by the reader.
fn adopt_legacy_layers(panels: &mut [Panel], legacy: Vec<LegacyPanelStates>) {
    for entry in legacy {
        let Some(panel) = panels
            .iter_mut()
            .find(|panel| panel.panel_id == entry.panel_id)
        else {
            continue;
        };
        for control in entry.controls {
            let Some(target) = panel
                .controls
                .iter_mut()
                .find(|existing| existing.control_id == control.control_id)
            else {
                continue;
            };
            if target.default_state.layers.is_empty() {
                target.default_state.layers = control.default_state.into_layers();
            }
            if let (Some(pressed), Some(legacy)) =
                (target.pressed_state.as_mut(), control.pressed_state)
            {
                if pressed.layers.is_empty() {
                    pressed.layers = legacy.into_layers();
                }
            }
        }
    }
}

pub fn save(path: &Path, panels: Vec<Panel>) -> Result<()> {
    write_toml(path, &document(panels))
}

pub fn render(panels: Vec<Panel>) -> Result<String> {
    Ok(toml::to_string_pretty(&document(panels))?)
}

fn document(panels: Vec<Panel>) -> PanelsDocument {
    PanelsDocument {
        version: PANEL_DOCUMENT_VERSION,
        panels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::defaults::default_panel;

    const LEGACY_DOCUMENT: &str = r#"
version = 3

[[panels]]
panel_id = "studio-panel-1"
name = "Hello"
controls = []
dial_ring_levels = [90]

[panels.layout]
columns = 16
rows = 2

[[panels.dial_colors]]
red = 35
green = 88
blue = 165
alpha = 255

[[panels.dial_colors]]
red = 7
green = 37
blue = 85
alpha = 255
"#;

    #[test]
    fn reads_the_dials_of_a_pre_version_4_panel_from_its_parallel_arrays() {
        let panels = parse(LEGACY_DOCUMENT).expect("a version 3 document should still load");
        let dials = &panels[0].dials;

        assert_eq!(dials.len(), 2);
        assert_eq!((dials[0].index, dials[0].level), (0, 90));
        assert_eq!(dials[0].color.red, 35);
        // A colour with no matching level starts from a full ring.
        assert_eq!((dials[1].index, dials[1].level), (1, 100));
    }

    #[test]
    fn declared_dials_survive_a_round_trip_through_the_document() {
        let rendered = render(vec![default_panel()]).expect("panels should serialise");
        let panels = parse(&rendered).expect("what we write should parse back");

        assert_eq!(panels[0].dials, default_panel().dials);
    }

    /// The order matters as much as the contents: a migrated key has to draw the same picture, and
    /// the picture is the order.
    #[test]
    fn a_version_four_state_becomes_the_stack_that_draws_it() {
        let panels = parse(
            r#"
version = 4

[[panels]]
panel_id = "panel"
name = "Panel"
dials = []

[panels.layout]
columns = 1
rows = 1

[panels.capabilities]
supports_color = true
supports_images = true
supports_text = true
supports_brightness = false
supports_haptics = false

[[panels.controls]]
control_id = "key"
name = "Key"
action_bindings = []
text = "ignored"
image = "hash:art"

[panels.controls.position]
column = 0
row = 0

[panels.controls.default_state]
text = "Playing"
image = "hash:art"
is_pressed = false

[panels.controls.default_state.background_color]
red = 1
green = 2
blue = 3
alpha = 255

[panels.controls.default_state.content_layout]
text_anchor = "bottom_center"

[panels.controls.default_state.border]
color = "$(hass.home:light.colour)"
width = 4

[panels.controls.default_state.progress]
value = 3
maximum_value = 10

[panels.controls.default_state.overlay_image]
image = "hash:badge"
anchor = "bottom_end"
scale_percent = 32
"#,
        )
        .expect("a version four document");

        assert_eq!(
            panels[0].controls[0].default_state.layers,
            vec![
                Layer::Fill {
                    color: RgbaColor::opaque(1, 2, 3).into(),
                },
                Layer::Image {
                    image: AssetId("hash:art".to_string()),
                    fit: Fit::Cover,
                    anchor: Anchor9::Center,
                    scale_percent: 100,
                    tint: None,
                },
                Layer::Fill {
                    color: RgbaColor {
                        red: 0,
                        green: 0,
                        blue: 0,
                        alpha: ART_SCRIM_ALPHA,
                    }
                    .into(),
                },
                Layer::Text {
                    text: "Playing".to_string(),
                    color: RgbaColor::opaque(255, 255, 255).into(),
                    anchor: Anchor9::BottomCenter,
                    font_family: None,
                    font_size: None,
                },
                Layer::Bar {
                    value: 3.into(),
                    maximum: 10.into(),
                    color: RgbaColor::opaque(255, 255, 255).into(),
                    edge: Edge::Bottom,
                    thickness: 6,
                },
                Layer::Image {
                    image: AssetId("hash:badge".to_string()),
                    fit: Fit::Contain,
                    anchor: Anchor9::BottomEnd,
                    scale_percent: 32,
                    tint: None,
                },
                Layer::Border {
                    color: ColorBinding::Reference("$(hass.home:light.colour)".to_string()),
                    width: 4,
                },
            ]
        );
    }

    /// Art was only darkened when there was a label over it, so a picture on its own must not gain
    /// a scrim it never had.
    #[test]
    fn a_picture_with_no_label_keeps_its_brightness() {
        let panels = parse(&version_four_state(
            "image = \"hash:art\"\nis_pressed = false",
        ))
        .expect("a version four document");

        assert_eq!(
            panels[0].controls[0].default_state.layers,
            vec![Layer::Image {
                image: AssetId("hash:art".to_string()),
                fit: Fit::Cover,
                anchor: Anchor9::Center,
                scale_percent: 100,
                tint: None,
            }]
        );
    }

    #[test]
    fn a_version_five_document_is_not_second_guessed_by_the_legacy_reader() {
        let rendered = render(
            parse(&version_four_state("text = \"Hello\"\nis_pressed = false")).expect("migrates"),
        )
        .expect("re-renders");

        assert!(rendered.starts_with("version = 5"));
        assert_eq!(
            parse(&rendered).expect("round trips"),
            parse(&version_four_state("text = \"Hello\"\nis_pressed = false")).expect("migrates")
        );
    }

    fn version_four_state(state: &str) -> String {
        format!(
            r#"
version = 4

[[panels]]
panel_id = "panel"
name = "Panel"
dials = []

[panels.layout]
columns = 1
rows = 1

[panels.capabilities]
supports_color = true
supports_images = true
supports_text = true
supports_brightness = false
supports_haptics = false

[[panels.controls]]
control_id = "key"
name = "Key"
action_bindings = []

[panels.controls.position]
column = 0
row = 0

[panels.controls.default_state]
{state}
"#
        )
    }
}
