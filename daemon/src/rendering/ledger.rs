use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    sync::RwLock,
};

use crate::surfaces::command::KeyRendering;

/// What each key was last told to show. A repaint that resolves to the same thing costs a hash
/// lookup instead of a JPEG encode, which is what makes a live variable affordable.
#[derive(Default)]
pub struct RenderLedger {
    last_rendered: RwLock<HashMap<(String, u8), u64>>,
}

impl RenderLedger {
    /// Records what a key is about to be told to show, and answers whether that differs from what
    /// it was last told. Without this a single one-hertz variable on a 16x2 Studio would re-encode
    /// thirty-two JPEGs a second to change one of them.
    pub fn record(&self, surface_id: &str, rendering: &KeyRendering) -> bool {
        let fingerprint = fingerprint(rendering);
        self.last_rendered
            .write()
            .unwrap()
            .insert((surface_id.to_string(), rendering.key_index), fingerprint)
            != Some(fingerprint)
    }

    /// Forgets what a surface was showing, so the next repaint is sent rather than deduplicated
    /// against a device that has since been reset.
    /// Forgets everything. Needed when what changed is not the rendering itself but what an id
    /// resolves to — a fetched image landing leaves the `KeyRendering` byte-identical, so without
    /// this the repaint that would finally draw the picture is dropped as a duplicate.
    pub fn forget_all(&self) {
        self.last_rendered.write().unwrap().clear();
    }

    pub fn forget(&self, surface_id: &str) {
        self.last_rendered
            .write()
            .unwrap()
            .retain(|(id, _), _| id != surface_id);
    }
}

fn fingerprint(rendering: &KeyRendering) -> u64 {
    let mut hasher = DefaultHasher::new();
    rendering.hash(&mut hasher);
    hasher.finish()
}
