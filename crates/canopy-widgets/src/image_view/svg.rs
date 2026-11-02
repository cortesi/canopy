//! SVG drawings, parsed once and rasterized at the size each caller asks for.

use std::{
    fmt,
    fs::File,
    io::{Cursor, Read},
    path::Path,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use canopy::{
    error::{Error, Result},
    geom::Size,
};
use image::{ImageReader, Rgba, RgbaImage};
use resvg::{
    tiny_skia::{Pixmap, Transform},
    usvg::{ImageHrefResolver, ImageKind, Options, Tree, fontdb::Database},
};

use super::PREVIEW_MAX_PIXELS;

/// Largest drawing source read, in bytes.
const SOURCE_LIMIT: u64 = 16 * 1024 * 1024;

/// Report a failure to read, parse, or draw a drawing.
fn svg_error(err: impl fmt::Display) -> Error {
    Error::Invalid(format!("svg error: {err}"))
}

/// Return whether `path` names an SVG drawing by its extension.
pub fn is_svg_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
}

/// The system fonts, which text in a drawing renders with. They load once,
/// on the first drawing that a process parses.
fn fonts() -> Arc<Database> {
    static FONTS: OnceLock<Arc<Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut fonts = Database::new();
            fonts.load_system_fonts();
            Arc::new(fonts)
        })
        .clone()
}

/// Parse a drawing from its source. Images that the drawing embeds as data
/// load, within 32 * 1024 * 1024 pixels for all of them together, each use
/// counted; files that it links to are never read. Gzipped data, which
/// could expand without bound, does not parse.
pub(super) fn parse(data: &[u8]) -> Result<Tree> {
    let embedded = ImageHrefResolver::default_data_resolver();
    let budget = AtomicU64::new(PREVIEW_MAX_PIXELS);
    let options = Options {
        fontdb: fonts(),
        image_href_resolver: ImageHrefResolver {
            resolve_data: Box::new(|mime, data, options| {
                let image = embedded(mime, data, options)?;
                let pixels = match &image {
                    ImageKind::SVG(_) => 0,
                    ImageKind::JPEG(data)
                    | ImageKind::PNG(data)
                    | ImageKind::GIF(data)
                    | ImageKind::WEBP(data) => raster_pixels(data)?,
                };
                budget
                    .try_update(Ordering::Relaxed, Ordering::Relaxed, |left| {
                        left.checked_sub(pixels)
                    })
                    .ok()?;
                Some(image)
            }),
            resolve_string: Box::new(|_, _| None),
        },
        ..Options::default()
    };
    Tree::from_data(data, &options).map_err(svg_error)
}

/// The pixels of an embedded raster image, from its header, or none for
/// data that does not decode.
fn raster_pixels(data: &[u8]) -> Option<u64> {
    let (width, height) = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    Some(u64::from(width) * u64::from(height))
}

/// Read and parse the drawing at `path`, a source of at most 16 MiB.
pub(super) fn read(path: &Path) -> Result<Tree> {
    let mut data = Vec::new();
    File::open(path)
        .and_then(|file| file.take(SOURCE_LIMIT + 1).read_to_end(&mut data))
        .map_err(svg_error)?;
    if data.len() as u64 > SOURCE_LIMIT {
        return Err(svg_error(format!(
            "drawings are limited to {SOURCE_LIMIT} bytes"
        )));
    }
    parse(&data)
}

/// Rasterize `tree` to fit `bounds` pixels, preserving its aspect ratio. A
/// drawing larger than `bounds` shrinks to fit; a smaller one keeps its own
/// size unless `grow` is set, when it grows to fit.
pub(super) fn rasterize(tree: &Tree, bounds: Size, grow: bool) -> Result<RgbaImage> {
    let size = tree.size();
    let (width, height) = (size.width(), size.height());
    let mut scale = (bounds.w as f32 / width).min(bounds.h as f32 / height);
    if !grow {
        scale = scale.min(1.0);
    }
    let pixels = |edge: f32, bound: u32| ((edge * scale).round() as u32).clamp(1, bound);
    let (w, h) = (pixels(width, bounds.w), pixels(height, bounds.h));
    let mut pixmap = Pixmap::new(w, h).ok_or_else(|| svg_error("empty raster"))?;
    let transform = Transform::from_scale(w as f32 / width, h as f32 / height);
    resvg::render(tree, transform, &mut pixmap.as_mut());
    let mut image = RgbaImage::new(w, h);
    for (out, pixel) in image.pixels_mut().zip(pixmap.pixels()) {
        let color = pixel.demultiply();
        *out = Rgba([color.red(), color.green(), color.blue(), color.alpha()]);
    }
    Ok(image)
}
