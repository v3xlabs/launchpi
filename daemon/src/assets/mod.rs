use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use image::{imageops::FilterType, RgbImage};
use sha2::{Digest, Sha256};
use tracing::debug;

use crate::identifiers::AssetId;

/// How many decoded and resized images stay in memory. Album art arrives at 640x640 and is drawn
/// at 96x96; decoding it on every repaint would be pure waste, and a handful of tracks is all a
/// panel ever shows at once.
const DECODED_CACHE_SIZE: usize = 32;

/// Content-addressed storage for anything a key can display.
///
/// Plugins put bytes in and get an id back; the render path only ever reads. That split is what
/// keeps network I/O out of rendering entirely — a value only changes once its bytes are on disk,
/// so a key never has to repaint a second time when a download lands.
pub struct AssetStore {
    root: PathBuf,
    decoded: Mutex<DecodedCache>,
}

impl AssetStore {
    pub fn open(cache_directory: PathBuf) -> Result<Self> {
        fs::create_dir_all(&cache_directory)
            .with_context(|| format!("unable to create {}", cache_directory.display()))?;
        Ok(Self {
            root: cache_directory,
            decoded: Mutex::new(DecodedCache::new(DECODED_CACHE_SIZE)),
        })
    }

    /// Storing identical bytes twice yields the same id, so a player re-announcing the same
    /// artwork changes no value and repaints nothing.
    pub fn insert_bytes(&self, bytes: &[u8]) -> Result<AssetId> {
        let digest = format!("{:x}", Sha256::digest(bytes));
        let path = self.path_for(&digest);
        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let temporary = path.with_extension("part");
            fs::write(&temporary, bytes)?;
            fs::rename(temporary, &path)?;
        }
        Ok(AssetId(format!("hash:{digest}")))
    }

    /// Decoded and resized to cover a square of `size`, centre-cropped. `None` covers every reason
    /// an image might not be drawable — unknown id, missing file, corrupt bytes — because all of
    /// them mean the same thing to the renderer: draw the key without a picture.
    pub fn decoded(&self, asset: &AssetId, size: u32) -> Option<Arc<RgbImage>> {
        let digest = asset.0.strip_prefix("hash:")?;
        let key = (digest.to_string(), size);

        if let Some(hit) = self.decoded.lock().unwrap().get(&key) {
            return Some(hit);
        }

        let bytes = fs::read(self.path_for(digest)).ok()?;
        let decoded = match image::load_from_memory(&bytes) {
            Ok(decoded) => decoded,
            Err(error) => {
                debug!(digest, %error, "stored asset could not be decoded");
                return None;
            }
        };
        let covered = Arc::new(cover(&decoded.to_rgb8(), size));
        self.decoded.lock().unwrap().insert(key, covered.clone());
        Some(covered)
    }

    /// Sharded by the first byte so one directory does not accumulate every asset ever seen.
    fn path_for(&self, digest: &str) -> PathBuf {
        self.root.join(&digest[..2]).join(digest)
    }
}

/// Scales so the shorter side fills `size`, then centre-crops. Letterboxing would leave bars in
/// whatever the background happens to be, which looks like a bug rather than a choice.
fn cover(source: &RgbImage, size: u32) -> RgbImage {
    let (width, height) = source.dimensions();
    if width == 0 || height == 0 {
        return RgbImage::new(size, size);
    }
    let scale = f64::from(size) / f64::from(width.min(height));
    let scaled_width = ((f64::from(width) * scale).round() as u32).max(size);
    let scaled_height = ((f64::from(height) * scale).round() as u32).max(size);
    let scaled = image::imageops::resize(source, scaled_width, scaled_height, FilterType::Triangle);

    image::imageops::crop_imm(
        &scaled,
        (scaled_width - size) / 2,
        (scaled_height - size) / 2,
        size,
        size,
    )
    .to_image()
}

struct DecodedCache {
    capacity: usize,
    entries: HashMap<(String, u32), Arc<RgbImage>>,
    order: VecDeque<(String, u32)>,
}

impl DecodedCache {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&mut self, key: &(String, u32)) -> Option<Arc<RgbImage>> {
        let hit = self.entries.get(key)?.clone();
        self.order.retain(|existing| existing != key);
        self.order.push_back(key.clone());
        Some(hit)
    }

    fn insert(&mut self, key: (String, u32), value: Arc<RgbImage>) {
        if self.entries.insert(key.clone(), value).is_none() {
            self.order.push_back(key);
        }
        while self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (AssetStore, tempdir::TempDir) {
        let directory = tempdir::TempDir::new();
        let store = AssetStore::open(directory.path().to_path_buf()).expect("opens");
        (store, directory)
    }

    /// The crate has no dev-dependency on a temp-dir helper, and adding one for four tests is not
    /// worth a dependency review.
    mod tempdir {
        use std::path::{Path, PathBuf};

        pub struct TempDir(PathBuf);

        impl TempDir {
            pub fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "launchpi-assets-{}-{:?}",
                    std::process::id(),
                    std::thread::current().id()
                ));
                let _ = std::fs::remove_dir_all(&path);
                std::fs::create_dir_all(&path).expect("creates a temporary directory");
                Self(path)
            }

            pub fn path(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut image = RgbImage::new(width, height);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = image::Rgb([(x % 256) as u8, (y % 256) as u8, 128]);
        }
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .expect("encodes");
        bytes.into_inner()
    }

    #[test]
    fn identical_bytes_yield_the_same_id() {
        let (store, _guard) = store();
        let bytes = png(8, 8);

        let first = store.insert_bytes(&bytes).expect("stores");
        let second = store.insert_bytes(&bytes).expect("stores");

        assert_eq!(first, second);
        assert!(first.0.starts_with("hash:"));
    }

    #[test]
    fn different_bytes_yield_different_ids() {
        let (store, _guard) = store();

        let first = store.insert_bytes(&png(8, 8)).expect("stores");
        let second = store.insert_bytes(&png(9, 9)).expect("stores");

        assert_ne!(first, second);
    }

    #[test]
    fn a_stored_image_decodes_to_the_requested_square() {
        let (store, _guard) = store();
        let asset = store.insert_bytes(&png(120, 60)).expect("stores");

        let decoded = store.decoded(&asset, 96).expect("decodes");

        assert_eq!(decoded.dimensions(), (96, 96));
    }

    #[test]
    fn an_unknown_or_corrupt_asset_decodes_to_nothing_rather_than_failing() {
        let (store, _guard) = store();

        assert!(store
            .decoded(&AssetId("hash:deadbeef".to_string()), 96)
            .is_none());
        assert!(store
            .decoded(&AssetId("builtin:play".to_string()), 96)
            .is_none());

        let corrupt = store.insert_bytes(b"not an image").expect("stores");
        assert!(store.decoded(&corrupt, 96).is_none());
    }

    #[test]
    fn covering_a_wide_image_crops_rather_than_letterboxes() {
        let wide = image::load_from_memory(&png(200, 50))
            .expect("decodes")
            .to_rgb8();

        let covered = cover(&wide, 96);

        assert_eq!(covered.dimensions(), (96, 96));
    }

    #[test]
    fn the_decoded_cache_evicts_the_oldest_entry() {
        let mut cache = DecodedCache::new(2);
        let image = || Arc::new(RgbImage::new(1, 1));

        cache.insert(("a".to_string(), 96), image());
        cache.insert(("b".to_string(), 96), image());
        cache.get(&("a".to_string(), 96));
        cache.insert(("c".to_string(), 96), image());

        assert!(
            cache.get(&("a".to_string(), 96)).is_some(),
            "recently used survives"
        );
        assert!(
            cache.get(&("b".to_string(), 96)).is_none(),
            "oldest is evicted"
        );
        assert!(cache.get(&("c".to_string(), 96)).is_some());
    }
}
