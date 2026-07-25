use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use image::{
    imageops::FilterType, DynamicImage, ImageBuffer, Pixel, Rgb, RgbImage, Rgba, RgbaImage,
};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tracing::debug;

use crate::identifiers::AssetId;

pub mod icons;

/// A URL that failed is not retried until this passes. Without it a broken link would be re-fetched
/// on every repaint of the key showing it.
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(60);

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
    decoded: Mutex<DecodedCache<Rgb<u8>>>,
    /// A second cache, because the overlay layer is the only thing that needs an alpha channel and
    /// the main art path must keep handing the renderer a buffer it can `copy_from_slice`.
    decoded_rgba: Mutex<DecodedCache<Rgba<u8>>>,
    http: reqwest::Client,
    /// URLs currently being fetched, and when each last failed. A panel repainting thirty-two keys
    /// that all show the same avatar must produce one download, not thirty-two.
    in_flight: Mutex<HashSet<String>>,
    failed: Mutex<HashMap<String, Instant>>,
    /// Poked when bytes land, so whatever is on screen can be drawn again with the picture.
    ready: mpsc::Sender<String>,
}

impl AssetStore {
    pub fn open(
        cache_directory: PathBuf,
        http: reqwest::Client,
        ready: mpsc::Sender<String>,
    ) -> Result<Self> {
        fs::create_dir_all(&cache_directory)
            .with_context(|| format!("unable to create {}", cache_directory.display()))?;
        Ok(Self {
            root: cache_directory,
            decoded: Mutex::new(DecodedCache::new(DECODED_CACHE_SIZE)),
            decoded_rgba: Mutex::new(DecodedCache::new(DECODED_CACHE_SIZE)),
            http,
            in_flight: Mutex::default(),
            failed: Mutex::default(),
            ready,
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
    pub fn decoded(self: &Arc<Self>, asset: &AssetId, size: u32) -> Option<Arc<RgbImage>> {
        let key = (self.digest_of(asset)?, size);
        if let Some(hit) = self.decoded.lock().unwrap().get(&key) {
            return Some(hit);
        }
        let covered = Arc::new(cover(
            &self.decode_stored(asset, &key.0, size)?.to_rgb8(),
            size,
        ));
        self.decoded.lock().unwrap().insert(key, covered.clone());
        Some(covered)
    }

    /// The same picture with its alpha channel intact, and fitted inside the square rather than
    /// cropped to fill it. A badge is a shape on a transparent field: cropping would clip it and
    /// flattening would put an opaque box over whatever it is meant to annotate.
    pub fn decoded_rgba(self: &Arc<Self>, asset: &AssetId, size: u32) -> Option<Arc<RgbaImage>> {
        let key = (self.digest_of(asset)?, size);
        if let Some(hit) = self.decoded_rgba.lock().unwrap().get(&key) {
            return Some(hit);
        }
        let fitted = Arc::new(fit(
            &self.decode_stored(asset, &key.0, size)?.to_rgba8(),
            size,
        ));
        self.decoded_rgba
            .lock()
            .unwrap()
            .insert(key, fitted.clone());
        Some(fitted)
    }

    /// `size` is passed down because an SVG has no pixels of its own: it is drawn for the square it
    /// is wanted at, rather than drawn once and resampled like a photograph.
    fn decode_stored(
        self: &Arc<Self>,
        asset: &AssetId,
        digest: &str,
        size: u32,
    ) -> Option<DynamicImage> {
        if icons::is_icon(&asset.0) {
            return icons::rasterise(&asset.0, size).map(DynamicImage::ImageRgba8);
        }
        let bytes = match fs::read(self.path_for(digest)) {
            Ok(bytes) => bytes,
            Err(_) => {
                // Not stored yet. For a URL that means "go and get it", and the key redraws when
                // it lands; for anything else it means there is nothing to draw.
                self.fetch_in_background(asset);
                return None;
            }
        };
        if icons::looks_like_svg(&bytes) {
            return icons::rasterise_document(&bytes, size).map(DynamicImage::ImageRgba8);
        }
        match image::load_from_memory(&bytes) {
            Ok(decoded) => Some(decoded),
            Err(error) => {
                debug!(digest, %error, "stored asset could not be decoded");
                None
            }
        }
    }

    /// Where an id's bytes live. A `hash:` id names its own digest; a URL and an icon are both keyed
    /// by the digest of the id itself, so the same picture referenced twice is stored and decoded
    /// once.
    fn digest_of(&self, asset: &AssetId) -> Option<String> {
        if let Some(digest) = asset.0.strip_prefix("hash:") {
            return Some(digest.to_string());
        }
        (is_fetchable(&asset.0) || icons::is_icon(&asset.0))
            .then(|| format!("{:x}", Sha256::digest(asset.0.as_bytes())))
    }

    /// Downloads a URL once and stores it under the digest of the URL. Anything already in flight,
    /// recently failed, or not a URL at all is left alone.
    fn fetch_in_background(self: &Arc<Self>, asset: &AssetId) {
        let url = asset.0.clone();
        if !is_fetchable(&url) {
            return;
        }
        {
            let mut failed = self.failed.lock().unwrap();
            if failed
                .get(&url)
                .is_some_and(|at| at.elapsed() < RETRY_AFTER_FAILURE)
            {
                return;
            }
            failed.remove(&url);
        }
        if !self.in_flight.lock().unwrap().insert(url.clone()) {
            return;
        }
        // Rendering happens inside the runtime in the daemon, but not in a plain unit test.
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            self.in_flight.lock().unwrap().remove(&url);
            return;
        };

        let store = self.clone();
        handle.spawn(async move {
            let outcome = store.load(&url).await;
            store.in_flight.lock().unwrap().remove(&url);

            match outcome {
                Ok(bytes) => match store.insert_bytes_at(&url, &bytes) {
                    Ok(()) => {
                        let _ = store.ready.try_send(url.clone());
                    }
                    Err(error) => debug!(url, %error, "could not store a fetched image"),
                },
                Err(reason) => {
                    debug!(url, reason, "could not fetch an image");
                    store.failed.lock().unwrap().insert(url, Instant::now());
                }
            }
        });
    }

    async fn load(&self, url: &str) -> Result<Vec<u8>, String> {
        if let Some(path) = url.strip_prefix("file://") {
            return tokio::fs::read(percent_decoded(path))
                .await
                .map_err(|error| error.to_string());
        }
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("answered {}", response.status().as_u16()));
        }
        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|error| error.to_string())
    }

    /// Stores bytes under the digest of the URL rather than of the bytes, so the next render of
    /// that URL finds them without another request.
    fn insert_bytes_at(&self, url: &str, bytes: &[u8]) -> Result<()> {
        let digest = format!("{:x}", Sha256::digest(url.as_bytes()));
        let path = self.path_for(&digest);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("part");
        fs::write(&temporary, bytes)?;
        fs::rename(temporary, &path)?;
        Ok(())
    }

    /// Sharded by the first byte so one directory does not accumulate every asset ever seen.
    fn path_for(&self, digest: &str) -> PathBuf {
        self.root.join(&digest[..2]).join(digest)
    }
}

/// Which ids the daemon will go and get. Anything else — a `builtin:` shape, a bare string — is
/// something the renderer either already understands or cannot use.
fn is_fetchable(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://") || value.starts_with("file://")
}

/// `file://` URLs arrive percent-encoded, and pictures live in directories with spaces in them
/// more often than not.
fn percent_decoded(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' && at + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&value[at + 1..at + 3], 16) {
                out.push(byte);
                at += 3;
                continue;
            }
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Scales so the shorter side fills `size`, then centre-crops. Letterboxing would leave bars in
/// whatever the background happens to be, which looks like a bug rather than a choice.
fn cover<P>(source: &ImageBuffer<P, Vec<u8>>, size: u32) -> ImageBuffer<P, Vec<u8>>
where
    P: Pixel<Subpixel = u8> + 'static,
{
    let (width, height) = source.dimensions();
    if width == 0 || height == 0 {
        return ImageBuffer::new(size, size);
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

/// Scales so the longer side fits `size` and centres the result on a transparent square. The
/// counterpart to [`cover`], for pictures whose shape carries the meaning.
fn fit(source: &RgbaImage, size: u32) -> RgbaImage {
    let (width, height) = source.dimensions();
    if width == 0 || height == 0 {
        return RgbaImage::new(size, size);
    }
    let scale = f64::from(size) / f64::from(width.max(height));
    let scaled_width = ((f64::from(width) * scale).round() as u32).clamp(1, size);
    let scaled_height = ((f64::from(height) * scale).round() as u32).clamp(1, size);
    let scaled = image::imageops::resize(source, scaled_width, scaled_height, FilterType::Triangle);

    let mut fitted = RgbaImage::new(size, size);
    image::imageops::overlay(
        &mut fitted,
        &scaled,
        i64::from((size - scaled_width) / 2),
        i64::from((size - scaled_height) / 2),
    );
    fitted
}

type CachedImage<P> = Arc<ImageBuffer<P, Vec<<P as Pixel>::Subpixel>>>;

struct DecodedCache<P: Pixel> {
    capacity: usize,
    entries: HashMap<(String, u32), CachedImage<P>>,
    order: VecDeque<(String, u32)>,
}

impl<P: Pixel> DecodedCache<P> {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&mut self, key: &(String, u32)) -> Option<CachedImage<P>> {
        let hit = self.entries.get(key)?.clone();
        self.order.retain(|existing| existing != key);
        self.order.push_back(key.clone());
        Some(hit)
    }

    fn insert(&mut self, key: (String, u32), value: CachedImage<P>) {
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

    fn store() -> (Arc<AssetStore>, tempdir::TempDir) {
        let directory = tempdir::TempDir::new();
        let (ready, _receiver) = mpsc::channel::<String>(8);
        let store = AssetStore::open(
            directory.path().to_path_buf(),
            reqwest::Client::new(),
            ready,
        )
        .expect("opens");
        (Arc::new(store), directory)
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
    fn a_url_is_keyed_by_the_url_so_the_same_picture_is_stored_once() {
        let (store, _guard) = store();
        let url = AssetId("https://example.test/cover.png".to_string());

        let digest = store.digest_of(&url).expect("a url has a digest");
        assert_eq!(store.digest_of(&url), Some(digest.clone()));
        assert_ne!(
            store.digest_of(&AssetId("https://example.test/other.png".to_string())),
            Some(digest)
        );
    }

    #[test]
    fn only_fetchable_ids_are_gone_after() {
        assert!(is_fetchable("https://example.test/a.png"));
        assert!(is_fetchable("http://example.test/a.png"));
        assert!(is_fetchable("file:///home/luc/cover.jpg"));
        assert!(!is_fetchable("builtin:play"));
        assert!(!is_fetchable("hash:abc123"));
        assert!(!is_fetchable("mdi:lightbulb-on"));
        assert!(!is_fetchable("just some text"));
    }

    #[test]
    fn an_icon_decodes_at_each_size_it_is_asked_for() {
        let (store, _guard) = store();
        let icon = AssetId("mdi:lightbulb-on".to_string());

        let small = store.decoded_rgba(&icon, 32).expect("rasterises");
        let large = store.decoded_rgba(&icon, 96).expect("rasterises");

        assert_eq!(small.dimensions(), (32, 32));
        assert_eq!(large.dimensions(), (96, 96));
        assert!(small.pixels().any(|pixel| pixel.0[3] > 0));
        assert!(Arc::ptr_eq(
            &large,
            &store.decoded_rgba(&icon, 96).expect("rasterises")
        ));
        assert!(!Arc::ptr_eq(
            &large,
            &store.decoded_rgba(&icon, 32).unwrap()
        ));
    }

    #[test]
    fn an_unknown_icon_draws_nothing_and_is_never_fetched() {
        let (store, _guard) = store();
        let icon = AssetId("mdi:no-such-icon".to_string());

        assert!(store.decoded_rgba(&icon, 96).is_none());
        assert!(store.decoded(&icon, 96).is_none());
        assert!(store.in_flight.lock().unwrap().is_empty());
        assert!(store.failed.lock().unwrap().is_empty());
    }

    #[test]
    fn an_icon_is_not_mistaken_for_a_url() {
        let (store, _guard) = store();
        let icon = AssetId("mdi:lightbulb-on".to_string());

        let digest = store.digest_of(&icon).expect("an icon has a digest");
        assert_eq!(store.digest_of(&icon), Some(digest.clone()));
        assert_ne!(
            store.digest_of(&AssetId("mdi:lightbulb".to_string())),
            Some(digest.clone())
        );

        store.decoded_rgba(&icon, 96).expect("rasterises");
        assert!(!store.path_for(&digest).exists(), "nothing is fetched");
    }

    #[test]
    fn a_stored_svg_is_rasterised_rather_than_refused() {
        let (store, _guard) = store();
        let asset = store
            .insert_bytes(
                br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10" fill="#0f0"/></svg>"##,
            )
            .expect("stores");

        let decoded = store.decoded(&asset, 48).expect("rasterises");

        assert_eq!(decoded.dimensions(), (48, 48));
        assert_eq!(decoded.get_pixel(24, 24).0, [0, 255, 0]);
    }

    #[test]
    fn an_unfetched_url_draws_nothing_rather_than_blocking() {
        let (store, _guard) = store();

        // No runtime here, so nothing is spawned; the point is that it returns instead of waiting.
        assert!(store
            .decoded(&AssetId("https://example.test/cover.png".to_string()), 96)
            .is_none());
    }

    #[test]
    fn a_file_url_path_is_percent_decoded() {
        assert_eq!(
            percent_decoded("/home/luc/My%20Album/cover.jpg"),
            "/home/luc/My Album/cover.jpg"
        );
        assert_eq!(percent_decoded("/plain/path.png"), "/plain/path.png");
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
