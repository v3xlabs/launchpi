use std::sync::LazyLock;

use image::RgbaImage;
use resvg::{tiny_skia, usvg};

/// Vendored from the `@mdi/svg` npm package: one line per icon, its name then the `d` attribute of
/// its single path, sorted by name. Every Material Design Icon is one path on a `0 0 24 24` canvas,
/// so keeping the surrounding `<svg>` element would be storing the same preamble 7447 times.
const MDI: &str = include_str!("mdi.txt");

const MDI_PREFIX: &str = "mdi:";

static MDI_PATHS: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    MDI.lines()
        .filter_map(|line| line.split_once(' '))
        .collect()
});

/// Whether an id names an icon, regardless of whether that icon exists. A misspelled name is not a
/// URL and must never be gone after over the network.
pub fn is_icon(id: &str) -> bool {
    id.strip_prefix(MDI_PREFIX)
        .is_some_and(|name| !name.is_empty())
}

/// An icon drawn at exactly `size`, white on transparent. `None` for a name no pack has.
pub fn rasterise(id: &str, size: u32) -> Option<RgbaImage> {
    let name = id.strip_prefix(MDI_PREFIX)?;
    let paths = &*MDI_PATHS;
    let at = paths.binary_search_by_key(&name, |(icon, _)| *icon).ok()?;
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="#fff" d="{}"/></svg>"##,
        paths[at].1
    );

    let pixmap = render(svg.as_bytes(), size)?;
    // A later layer tints an icon by multiplying a colour through it, which only works on coverage:
    // the black glyph the pack ships would multiply every colour back to black.
    let mut pixels = Vec::with_capacity(pixmap.pixels().len() * 4);
    for pixel in pixmap.pixels() {
        pixels.extend_from_slice(&[255, 255, 255, pixel.alpha()]);
    }
    RgbaImage::from_raw(pixmap.width(), pixmap.height(), pixels)
}

/// An arbitrary SVG document drawn so its shorter side is `size`, leaving the caller's `cover` or
/// `fit` to crop or shrink rather than to enlarge.
pub fn rasterise_document(svg: &[u8], size: u32) -> Option<RgbaImage> {
    let pixmap = render(svg, size)?;
    let mut pixels = Vec::with_capacity(pixmap.pixels().len() * 4);
    for pixel in pixmap.pixels() {
        let colour = pixel.demultiply();
        pixels.extend_from_slice(&[colour.red(), colour.green(), colour.blue(), colour.alpha()]);
    }
    RgbaImage::from_raw(pixmap.width(), pixmap.height(), pixels)
}

/// Whether stored bytes are an SVG document rather than something [`image`] can decode.
pub fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(1024)];
    head.windows(4).any(|window| window == b"<svg")
}

fn render(svg: &[u8], size: u32) -> Option<tiny_skia::Pixmap> {
    if size == 0 {
        return None;
    }
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).ok()?;
    let source = tree.size();
    let scale = size as f32 / source.width().min(source.height());
    // A sliver of a viewBox would otherwise ask for a pixmap of gigabytes.
    let width = ((source.width() * scale).round() as u32).clamp(1, size * 4);
    let height = ((source.height() * scale).round() as u32).clamp(1, size * 4);

    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(pixmap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_line_of_the_pack_parses() {
        assert_eq!(MDI_PATHS.len(), MDI.lines().count());
        assert!(MDI_PATHS.len() > 7000);
    }

    #[test]
    fn the_pack_is_sorted_so_it_can_be_searched() {
        assert!(MDI_PATHS.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }

    #[test]
    fn an_icon_id_is_recognised_by_its_prefix_alone() {
        assert!(is_icon("mdi:lightbulb-on"));
        assert!(is_icon("mdi:no-such-icon"));
        assert!(!is_icon("mdi:"));
        assert!(!is_icon("hash:abc123"));
        assert!(!is_icon("https://example.test/a.svg"));
    }

    #[test]
    fn an_icon_rasterises_to_white_coverage_at_the_size_asked_for() {
        let small = rasterise("mdi:lightbulb-on", 32).expect("rasterises");
        let large = rasterise("mdi:lightbulb-on", 96).expect("rasterises");

        assert_eq!(small.dimensions(), (32, 32));
        assert_eq!(large.dimensions(), (96, 96));
        assert!(small.pixels().any(|pixel| pixel.0[3] > 0), "has coverage");
        assert!(
            large.pixels().all(|pixel| pixel.0[..3] == [255, 255, 255]),
            "a tint has something to multiply"
        );
        assert!(
            large.pixels().filter(|pixel| pixel.0[3] > 0).count()
                > small.pixels().filter(|pixel| pixel.0[3] > 0).count(),
            "drawn at the size asked for rather than scaled up"
        );
    }

    #[test]
    fn an_unknown_icon_rasterises_to_nothing() {
        assert!(rasterise("mdi:no-such-icon", 96).is_none());
        assert!(rasterise("si:discord", 96).is_none());
    }

    #[test]
    fn a_document_keeps_its_own_colours() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10" fill="#f00"/></svg>"##;

        let drawn = rasterise_document(svg, 8).expect("rasterises");

        assert_eq!(drawn.dimensions(), (8, 8));
        assert_eq!(drawn.get_pixel(4, 4).0, [255, 0, 0, 255]);
    }

    #[test]
    fn only_svg_bytes_look_like_svg() {
        assert!(looks_like_svg(
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"
        ));
        assert!(looks_like_svg(
            b"<?xml version=\"1.0\"?>\n<!-- a comment -->\n<svg/>"
        ));
        assert!(!looks_like_svg(b"\x89PNG\r\n\x1a\n"));
        assert!(!looks_like_svg(b""));
    }
}
