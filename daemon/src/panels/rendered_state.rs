use serde::{Deserialize, Serialize};

use crate::identifiers::AssetId;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RenderedState {
    pub text: Option<String>,
    pub image: Option<AssetId>,
    /// A badge drawn small in a corner. Kept apart from `image` because it is never scrimmed: a
    /// status marker that dims as soon as the key gains a label is not a status marker.
    pub overlay_image: Option<OverlayImage>,
    pub foreground_color: Option<ColorBinding>,
    pub background_color: Option<ColorBinding>,
    pub border: Option<Border>,
    pub progress: Option<Progress>,
    #[serde(default)]
    pub content_layout: ContentLayout,
    pub is_pressed: bool,
}

/// An inset outline. The colour binds like any other colour, so a plugin can drive it; the width
/// does not, because nothing publishes a width.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Border {
    pub color: ColorBinding,
    #[serde(default = "default_border_width")]
    pub width: u8,
}

fn default_border_width() -> u8 {
    5
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OverlayImage {
    pub image: AssetId,
    #[serde(default = "default_overlay_anchor")]
    pub anchor: Anchor9,
    #[serde(default = "default_overlay_scale")]
    pub scale_percent: u8,
}

fn default_overlay_anchor() -> Anchor9 {
    Anchor9::BottomEnd
}

fn default_overlay_scale() -> u8 {
    32
}

/// A [`Border`] and an [`OverlayImage`] with their bindings resolved, so the renderer never has to
/// know what a binding is.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ResolvedBorder {
    pub color: RgbaColor,
    pub width: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ResolvedOverlay {
    pub image: AssetId,
    pub anchor: Anchor9,
    pub scale_percent: u8,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ContentLayout {
    #[serde(default)]
    pub text_anchor: Anchor9,
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

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Progress {
    pub value: u16,
    pub maximum_value: u16,
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
    fn a_border_binds_its_colour_and_defaults_its_width() {
        #[derive(Deserialize)]
        struct Holder {
            border: Border,
        }

        let holder: Holder =
            toml::from_str("border = { color = \"$(discord.home:status)\" }").expect("valid toml");
        assert_eq!(
            holder.border.color,
            ColorBinding::Reference("$(discord.home:status)".to_string())
        );
        assert_eq!(holder.border.width, default_border_width());
    }

    /// The fields are additive, so nothing forces a `PANEL_DOCUMENT_VERSION` bump.
    #[test]
    fn a_control_written_before_borders_existed_still_parses() {
        let state: RenderedState =
            toml::from_str("text = \"Hello\"\nis_pressed = false").expect("valid toml");
        assert_eq!(state.border, None);
        assert_eq!(state.overlay_image, None);
    }

    #[test]
    fn an_overlay_takes_a_corner_and_a_size_unless_told_otherwise() {
        let overlay: OverlayImage =
            toml::from_str("image = \"$(discord.home:badge)\"").expect("valid toml");
        assert_eq!(overlay.anchor, default_overlay_anchor());
        assert_eq!(overlay.scale_percent, default_overlay_scale());
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
