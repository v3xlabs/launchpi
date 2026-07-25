use serde::{Deserialize, Serialize};

use crate::identifiers::AssetId;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RenderedState {
    pub text: Option<String>,
    pub image: Option<AssetId>,
    pub foreground_color: Option<ColorBinding>,
    pub background_color: Option<ColorBinding>,
    pub progress: Option<Progress>,
    pub is_pressed: bool,
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
