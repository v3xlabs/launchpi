use serde::{Deserialize, Serialize};

use crate::identifiers::SurfaceId;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Gesture {
    pub surface_id: SurfaceId,
    pub surface_control_id: String,
    pub kind: GestureKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum GestureKind {
    Press,
    Release,
    Hold { duration_ms: u64 },
    Repeat { interval_ms: u64 },
    Rotate { delta: i32 },
    SetValue { value: u16, maximum_value: u16 },
}
