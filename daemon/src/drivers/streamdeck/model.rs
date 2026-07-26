use serde::Serialize;

use crate::surfaces::layout::SurfaceLayout;

pub const STREAM_DECK_STUDIO_PRODUCT_ID: u16 = 0x00aa;
/// The dock is not a USB device, so it has no real product id; the protocol reports this one.
pub const NETWORK_DOCK_PRODUCT_ID: u16 = 0xffff;

/// Where a knob sits, in the key grid's own cell coordinates: `(0, 0)` is the top-left key, so a
/// negative column or a column past the last one puts the knob beside the keys rather than on one.
/// The Studio's knobs flank the keys and stand as tall as the whole block, hence `row_span`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct DialPlacement {
    pub index: u8,
    pub column: i16,
    pub row: i16,
    pub row_span: u16,
}

/// Everything the rest of the daemon is allowed to assume about a piece of Stream Deck hardware.
/// Written down once here; nothing else hardcodes a grid size or a dial count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamDeckModel {
    pub name: &'static str,
    pub product_ids: &'static [u16],
    pub layout: SurfaceLayout,
    pub dials: &'static [DialPlacement],
}

impl StreamDeckModel {
    pub fn has_dial(&self, index: u8) -> bool {
        self.dials.iter().any(|dial| dial.index == index)
    }
}

pub const STREAM_DECK: StreamDeckModel = StreamDeckModel {
    name: "Stream Deck",
    product_ids: &[0x0060, 0x006d],
    layout: SurfaceLayout::Grid {
        columns: 5,
        rows: 3,
    },
    dials: &[],
};

pub const STREAM_DECK_MINI: StreamDeckModel = StreamDeckModel {
    name: "Stream Deck Mini",
    product_ids: &[0x0063, 0x0090, 0x00b3],
    layout: SurfaceLayout::Grid {
        columns: 3,
        rows: 2,
    },
    dials: &[],
};

pub const STREAM_DECK_XL: StreamDeckModel = StreamDeckModel {
    name: "Stream Deck XL",
    product_ids: &[0x006c, 0x008f],
    layout: SurfaceLayout::Grid {
        columns: 8,
        rows: 4,
    },
    dials: &[],
};

pub const STREAM_DECK_MK2: StreamDeckModel = StreamDeckModel {
    name: "Stream Deck Mk.2",
    product_ids: &[0x0080, 0x00a5],
    layout: SurfaceLayout::Grid {
        columns: 5,
        rows: 3,
    },
    dials: &[],
};

pub const STREAM_DECK_PLUS: StreamDeckModel = StreamDeckModel {
    name: "Stream Deck Plus",
    product_ids: &[0x0084],
    layout: SurfaceLayout::Grid {
        columns: 4,
        rows: 2,
    },
    dials: &[
        DialPlacement {
            index: 0,
            column: 0,
            row: 2,
            row_span: 1,
        },
        DialPlacement {
            index: 1,
            column: 1,
            row: 2,
            row_span: 1,
        },
        DialPlacement {
            index: 2,
            column: 2,
            row: 2,
            row_span: 1,
        },
        DialPlacement {
            index: 3,
            column: 3,
            row: 2,
            row_span: 1,
        },
    ],
};

/// The Neo's two extra buttons are touch contacts, not rotary encoders, so it declares no dials.
pub const STREAM_DECK_NEO: StreamDeckModel = StreamDeckModel {
    name: "Stream Deck Neo",
    product_ids: &[0x009a],
    layout: SurfaceLayout::Grid {
        columns: 4,
        rows: 2,
    },
    dials: &[],
};

pub const STREAM_DECK_STUDIO: StreamDeckModel = StreamDeckModel {
    name: "Stream Deck Studio",
    product_ids: &[STREAM_DECK_STUDIO_PRODUCT_ID],
    layout: SurfaceLayout::Grid {
        columns: 16,
        rows: 2,
    },
    dials: &[
        DialPlacement {
            index: 0,
            column: -1,
            row: 0,
            row_span: 2,
        },
        DialPlacement {
            index: 1,
            column: 16,
            row: 0,
            row_span: 2,
        },
    ],
};

/// A hub rather than a surface: the Stream Deck plugged into it is registered as its own device.
pub const STREAM_DECK_NETWORK_DOCK: StreamDeckModel = StreamDeckModel {
    name: "Stream Deck Network Dock",
    product_ids: &[NETWORK_DOCK_PRODUCT_ID],
    layout: SurfaceLayout::Freeform,
    dials: &[],
};

pub const MODELS: &[StreamDeckModel] = &[
    STREAM_DECK,
    STREAM_DECK_MINI,
    STREAM_DECK_XL,
    STREAM_DECK_MK2,
    STREAM_DECK_PLUS,
    STREAM_DECK_NEO,
    STREAM_DECK_STUDIO,
    STREAM_DECK_NETWORK_DOCK,
];

/// An unrecognised product id is still an Elgato key grid, so it is treated as the original
/// Stream Deck rather than refused outright.
pub fn model_for_product_id(product_id: u16) -> &'static StreamDeckModel {
    MODELS
        .iter()
        .find(|model| model.product_ids.contains(&product_id))
        .unwrap_or(&STREAM_DECK)
}

/// Resolves the model a stored device was recorded as. `model` is a persisted string, so a device
/// written by a future version can name something this table does not have.
pub fn model_by_name(name: &str) -> Option<&'static StreamDeckModel> {
    MODELS.iter().find(|model| model.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_product_ids_to_the_correct_model_layout_and_dials() {
        let plus = model_for_product_id(0x0084);
        assert_eq!(plus.name, "Stream Deck Plus");
        assert_eq!(
            plus.layout,
            SurfaceLayout::Grid {
                columns: 4,
                rows: 2
            }
        );
        assert_eq!(plus.dials.len(), 4);
        assert!(plus.dials.iter().all(|dial| dial.row == 2));

        let studio = model_for_product_id(STREAM_DECK_STUDIO_PRODUCT_ID);
        assert_eq!(studio.name, "Stream Deck Studio");
        assert_eq!(
            studio.layout,
            SurfaceLayout::Grid {
                columns: 16,
                rows: 2
            }
        );
        assert_eq!(studio.dials[0].column, -1);
        assert_eq!(studio.dials[1].column, 16);

        let xl = model_for_product_id(0x006c);
        assert_eq!(xl.name, "Stream Deck XL");
        assert_eq!(
            xl.layout,
            SurfaceLayout::Grid {
                columns: 8,
                rows: 4
            }
        );
        assert!(xl.dials.is_empty());

        assert!(model_for_product_id(0x009a).dials.is_empty());
        assert_eq!(
            model_for_product_id(NETWORK_DOCK_PRODUCT_ID).name,
            "Stream Deck Network Dock"
        );
        assert_eq!(model_for_product_id(0xdead).name, "Stream Deck");
    }

    #[test]
    fn every_model_is_reachable_by_the_name_it_is_stored_under() {
        for model in MODELS {
            assert_eq!(model_by_name(model.name), Some(model));
            for product_id in model.product_ids {
                assert_eq!(model_for_product_id(*product_id).name, model.name);
            }
        }
        assert_eq!(model_by_name("Behringer X-Touch"), None);
    }

    #[test]
    fn a_dial_index_is_only_valid_when_the_model_declares_it() {
        assert!(STREAM_DECK_STUDIO.has_dial(1));
        assert!(!STREAM_DECK_STUDIO.has_dial(2));
        assert!(STREAM_DECK_PLUS.has_dial(3));
        assert!(!STREAM_DECK_XL.has_dial(0));
    }
}
