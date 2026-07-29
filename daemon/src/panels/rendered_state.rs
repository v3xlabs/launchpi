use serde::{Deserialize, Serialize};

use crate::identifiers::AssetId;

/// A key's face as a stack. Layers draw in array order, index 0 first, so later entries sit on top.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RenderedState {
    #[serde(default)]
    pub layers: Vec<Layer>,
    pub is_pressed: bool,
}

impl RenderedState {
    /// The stack a plain key starts from, and what the editor offers for a new one: a colour with a
    /// label on it. Two layers, so the simplest key stays as simple as it was before it had a stack.
    pub fn labelled(
        text: impl Into<String>,
        foreground: RgbaColor,
        background: RgbaColor,
        is_pressed: bool,
    ) -> Self {
        Self {
            layers: vec![
                Layer::Fill {
                    color: background.into(),
                },
                Layer::Text {
                    text: text.into(),
                    color: foreground.into(),
                    anchor: Anchor9::Center,
                    font_family: None,
                },
            ],
            is_pressed,
        }
    }
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
        #[serde(default)]
        tint: Option<ColorBinding>,
    },
    Text {
        text: String,
        color: ColorBinding,
        #[serde(default)]
        anchor: Anchor9,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        font_family: Option<String>,
    },
    Bar {
        value: ValueBinding,
        maximum: ValueBinding,
        color: ColorBinding,
        #[serde(default)]
        edge: Edge,
        #[serde(default = "default_bar_thickness")]
        thickness: u8,
    },
    Border {
        color: ColorBinding,
        #[serde(default = "default_border_width")]
        width: u8,
    },
}

/// `Cover` crops the picture to fill its square; `Contain` fits the whole picture inside it.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    #[default]
    Cover,
    Contain,
}

/// Which edge a [`Layer::Bar`] runs along and grows from.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    Top,
    #[default]
    Bottom,
    Start,
    End,
}

fn full_scale() -> u8 {
    100
}

fn default_bar_thickness() -> u8 {
    6
}

fn default_border_width() -> u8 {
    5
}

/// A [`Layer`] with every binding replaced by the value it names, so the renderer never has to know
/// what a binding is.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResolvedLayer {
    Fill {
        color: RgbaColor,
    },
    Image {
        image: AssetId,
        fit: Fit,
        anchor: Anchor9,
        scale_percent: u8,
        tint: Option<RgbaColor>,
    },
    Text {
        text: String,
        color: RgbaColor,
        anchor: Anchor9,
        font_family: String,
    },
    Bar {
        value: u16,
        maximum: u16,
        color: RgbaColor,
        edge: Edge,
        thickness: u8,
    },
    Border {
        color: RgbaColor,
        width: u8,
    },
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchor9 {
    TopStart,
    TopCenter,
    TopEnd,
    CenterStart,
    #[default]
    Center,
    CenterEnd,
    BottomStart,
    BottomCenter,
    BottomEnd,
}

/// A colour is either written out or read from a value. Untagged so both forms are natural TOML:
/// a table is a literal, a string is a reference. Order matters — a table can only be a literal.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ColorBinding {
    Literal(RgbaColor),
    Reference(String),
}

impl From<RgbaColor> for ColorBinding {
    fn from(color: RgbaColor) -> Self {
        Self::Literal(color)
    }
}

/// A number the same way: an integer is written out, a string reads one from a value.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ValueBinding {
    Literal(u16),
    Reference(String),
}

impl From<u16> for ValueBinding {
    fn from(value: u16) -> Self {
        Self::Literal(value)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct RgbaColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl RgbaColor {
    pub fn opaque(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 255,
        }
    }

    /// Accepts `#rgb`, `#rrggbb` and `#rrggbbaa`, with or without the leading hash. This is the
    /// wire format a plugin publishes a colour in, so it has to survive whatever an upstream API
    /// hands over.
    pub fn from_hex(value: &str) -> Option<Self> {
        let digits = value.trim().trim_start_matches('#');
        let byte = |at: usize| u8::from_str_radix(digits.get(at..at + 2)?, 16).ok();

        match digits.len() {
            3 => {
                let nibble = |at: usize| {
                    u8::from_str_radix(digits.get(at..at + 1)?, 16)
                        .ok()
                        .map(|value| value * 17)
                };
                Some(Self::opaque(nibble(0)?, nibble(1)?, nibble(2)?))
            }
            6 => Some(Self::opaque(byte(0)?, byte(2)?, byte(4)?)),
            8 => Some(Self {
                red: byte(0)?,
                green: byte(2)?,
                blue: byte(4)?,
                alpha: byte(6)?,
            }),
            _ => None,
        }
    }

    pub fn to_hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.red, self.green, self.blue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_table_reads_as_a_literal_and_a_string_as_a_reference() {
        #[derive(Deserialize)]
        struct Holder {
            color: ColorBinding,
        }

        let literal: Holder =
            toml::from_str("[color]\nred = 1\ngreen = 2\nblue = 3\nalpha = 4").expect("valid toml");
        assert_eq!(
            literal.color,
            ColorBinding::Literal(RgbaColor {
                red: 1,
                green: 2,
                blue: 3,
                alpha: 4
            })
        );

        let reference: Holder =
            toml::from_str("color = \"$(hass.home:light.color)\"").expect("valid toml");
        assert_eq!(
            reference.color,
            ColorBinding::Reference("$(hass.home:light.color)".to_string())
        );
    }

    #[test]
    fn a_layer_names_its_kind_and_defaults_the_rest() {
        let state: RenderedState = toml::from_str(
            r#"
is_pressed = false

[[layers]]
kind = "border"
color = "$(discord.home:status)"

[[layers]]
kind = "image"
image = "$(mpris.default:art)"
"#,
        )
        .expect("valid toml");

        assert_eq!(
            state.layers,
            vec![
                Layer::Border {
                    color: ColorBinding::Reference("$(discord.home:status)".to_string()),
                    width: default_border_width(),
                },
                Layer::Image {
                    image: AssetId("$(mpris.default:art)".to_string()),
                    fit: Fit::Cover,
                    anchor: Anchor9::Center,
                    scale_percent: full_scale(),
                    tint: None,
                },
            ]
        );
    }

    #[test]
    fn a_stack_survives_a_round_trip_through_toml() {
        let state = RenderedState {
            layers: vec![
                Layer::Fill {
                    color: RgbaColor::opaque(30, 41, 59).into(),
                },
                Layer::Text {
                    text: "$(mpris.default:title)".to_string(),
                    color: ColorBinding::Reference("$(mpris.default:accent)".to_string()),
                    anchor: Anchor9::BottomCenter,
                    font_family: None,
                },
                Layer::Bar {
                    value: ValueBinding::Reference("$(mpris.default:position)".to_string()),
                    maximum: 100.into(),
                    color: RgbaColor::opaque(255, 255, 255).into(),
                    edge: Edge::Bottom,
                    thickness: 6,
                },
            ],
            is_pressed: false,
        };

        let rendered = toml::to_string_pretty(&state).expect("a stack serialises");
        assert_eq!(
            toml::from_str::<RenderedState>(&rendered).expect("and reads back"),
            state
        );
    }

    #[test]
    fn a_state_with_no_stack_is_an_empty_stack_rather_than_an_error() {
        let state: RenderedState = toml::from_str("is_pressed = false").expect("valid toml");
        assert!(state.layers.is_empty());
    }

    #[test]
    fn hex_parses_in_every_length_upstreams_use() {
        assert_eq!(
            RgbaColor::from_hex("#e8b923"),
            Some(RgbaColor::opaque(232, 185, 35))
        );
        assert_eq!(
            RgbaColor::from_hex("e8b923"),
            Some(RgbaColor::opaque(232, 185, 35))
        );
        assert_eq!(
            RgbaColor::from_hex("#fff"),
            Some(RgbaColor::opaque(255, 255, 255))
        );
        assert_eq!(
            RgbaColor::from_hex("#01020304"),
            Some(RgbaColor {
                red: 1,
                green: 2,
                blue: 3,
                alpha: 4
            })
        );
    }

    #[test]
    fn nonsense_is_rejected_rather_than_rendered_as_black() {
        for value in ["", "#", "not a colour", "#12345", "#gggggg"] {
            assert_eq!(RgbaColor::from_hex(value), None, "{value} should not parse");
        }
    }

    #[test]
    fn hex_round_trips() {
        let color = RgbaColor::opaque(232, 185, 35);
        assert_eq!(RgbaColor::from_hex(&color.to_hex()), Some(color));
    }
}
