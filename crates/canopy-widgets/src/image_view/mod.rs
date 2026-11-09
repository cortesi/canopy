//! Image viewer widget with zoom and pan controls.

use std::{fmt, mem, path::Path, sync::Arc};

use canopy::{
    CommandEnum, Context, Register, Setup, ViewContext, Widget, derive_commands,
    error::{Error, Result},
    geom::{Point, Rect, Size},
    layout::{CanvasContext, Layout, ScrollDirection, ScrollOp},
    render::Render,
};
use canopy_mosaic::{
    Cell, RasterView, SymbolSet, Tier,
    raster::{Image, LinearRaster, Placement, composite, resample},
    rgb_into,
};
use image::{DynamicImage, ImageDecoder, RgbaImage};
use resvg::usvg::Tree;

use crate::mosaic::{self, cycle, tier_for};

mod svg;

pub use svg::is_svg_path;

/// Direction for zoom commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, CommandEnum)]
pub enum ZoomDirection {
    /// Zoom in.
    In,
    /// Zoom out.
    Out,
}

/// Minimum zoom factor.
const MIN_ZOOM: f32 = 0.1;
/// Maximum zoom factor.
const MAX_ZOOM: f32 = 16.0;
/// Zoom multiplier applied per step.
const ZOOM_STEP: f32 = 1.25;
/// Pan step in terminal columns.
const PAN_STEP_COLUMNS: i32 = 1;
/// Pan step in terminal rows.
const PAN_STEP_ROWS: i32 = 1;
/// Maximum source pixel count for thumbnail previews.
const PREVIEW_MAX_PIXELS: u64 = 32 * 1024 * 1024;
/// Decoder allocation budget for thumbnail previews, excluding conversion.
const PREVIEW_DECODE_BYTES: u64 = 128 * 1024 * 1024;
/// Largest retained thumbnail edge, regardless of caller-provided bounds.
const PREVIEW_MAX_EDGE: u32 = 1024;
/// Largest edge of a drawing read at full resolution, which has no pixels of
/// its own.
const DRAWING_MAX_EDGE: u32 = 4096;
/// Matrix pixels across and down a cell.
const MATRIX: usize = 8;

/// Report a failure to read or decode an image.
fn image_error(err: impl fmt::Display) -> Error {
    Error::Invalid(format!("image error: {err}"))
}

/// Decode an image file into RGBA pixels. A drawing keeps its own size,
/// within 4096 pixels an edge.
fn read_rgba(path: &Path) -> Result<RgbaImage> {
    if svg::is_svg_path(path) {
        return full_drawing(&svg::read(path)?);
    }
    let image = image::open(path).map_err(image_error)?;
    Ok(image.into_rgba8())
}

/// Rasterize a drawing at its own size, within 4096 pixels an edge.
fn full_drawing(tree: &Tree) -> Result<RgbaImage> {
    svg::rasterize(tree, Size::new(DRAWING_MAX_EDGE, DRAWING_MAX_EDGE), false)
}

/// Decode the image at `path` as a preview that fits `bounds` pixels, under
/// the limits of [`DecodedImage::read`] and 1024 pixels an edge, as
/// [`DecodedImage::scale`] does: a small drawing keeps its own size.
/// Callers should also bound the encoded file size.
pub fn read_preview(path: &Path, bounds: Size) -> Result<RgbaImage> {
    let bounds = preview_bounds(bounds)?;
    let preview = DecodedImage::read(path)?.scale(bounds)?;
    Ok(Arc::unwrap_or_clone(preview.image))
}

/// Return `bounds` with each edge capped at the largest preview edge, or an
/// error for an empty bound.
fn preview_bounds(bounds: Size) -> Result<Size> {
    if bounds.w == 0 || bounds.h == 0 {
        return Err(Error::Invalid(
            "image preview bounds must be nonzero".into(),
        ));
    }
    Ok(Size::new(
        bounds.w.min(PREVIEW_MAX_EDGE),
        bounds.h.min(PREVIEW_MAX_EDGE),
    ))
}

/// An image decoded for a preview, before it is scaled.
///
/// Reading and scaling are separate stages, so a worker that prepares
/// previews can give up on a stale request between them. The decode itself
/// cannot stop part way.
pub struct DecodedImage {
    /// The decoded source.
    source: Source,
}

/// What a decoded image holds.
enum Source {
    /// Pixels, in their source format.
    Raster(DynamicImage),
    /// An SVG drawing, which rasterizes at the size that a preview asks for.
    Vector(Box<Tree>),
}

impl DecodedImage {
    /// Decode the image at `path` under the preview limits: at most 32 *
    /// 1024 * 1024 source pixels and a 128 MiB decoder budget. A file named
    /// `.svg` parses as a drawing: a source of at most 16 MiB, which renders
    /// text with the system fonts, reads no file that it links to, and
    /// draws the images it embeds within the same pixel limit, all together.
    /// Callers should also bound the encoded file size.
    pub fn read(path: &Path) -> Result<Self> {
        if svg::is_svg_path(path) {
            return Ok(Self {
                source: Source::Vector(Box::new(svg::read(path)?)),
            });
        }
        let mut reader = image::ImageReader::open(path).map_err(image_error)?;
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(PREVIEW_DECODE_BYTES);
        reader.limits(limits.clone());
        let mut decoder = reader.into_decoder().map_err(image_error)?;
        let (width, height) = decoder.dimensions();
        if u64::from(width) * u64::from(height) > PREVIEW_MAX_PIXELS {
            return Err(Error::Invalid(format!(
                "image preview limited to {PREVIEW_MAX_PIXELS} pixels"
            )));
        }
        // ImageReader::decode reserves the output buffer before passing the
        // remaining budget to the decoder. Keep that accounting after
        // inspecting dimensions, so conversion and sampling cannot expand an
        // unchecked image.
        limits.reserve(decoder.total_bytes()).map_err(image_error)?;
        decoder.set_limits(limits).map_err(image_error)?;
        let image = DynamicImage::from_decoder(decoder).map_err(image_error)?;
        Ok(Self {
            source: Source::Raster(image),
        })
    }

    /// Scale the image to fit `bounds` pixels, preserving its aspect ratio,
    /// with each edge capped at 1024 pixels. Small images keep their pixels,
    /// and a small drawing keeps its own size. Zero bounds are invalid.
    pub fn scale(self, bounds: Size) -> Result<ImagePreview> {
        self.preview(bounds, false)
    }

    /// Scale the image as [`Self::scale`] does, except that a small drawing
    /// grows to fit `bounds`, so it stays sharp in a view that zooms it.
    pub fn fit(self, bounds: Size) -> Result<ImagePreview> {
        self.preview(bounds, true)
    }

    /// Scale the image to fit `bounds`, growing a drawing when `grow` is set.
    fn preview(self, bounds: Size, grow: bool) -> Result<ImagePreview> {
        let bounds = preview_bounds(bounds)?;
        let image = match self.source {
            Source::Raster(image) => thumbnail(image, bounds),
            Source::Vector(tree) => svg::rasterize(&tree, bounds, grow)?,
        };
        Ok(ImagePreview {
            image: Arc::new(image),
        })
    }
}

/// An image scaled for an image view, which [`ImageView::set_preview`]
/// shows. Clones share the pixels, so a cache of previews and the view that
/// shows one hold a single copy.
#[derive(Clone)]
pub struct ImagePreview {
    /// The scaled pixels, RGBA8 with unassociated alpha.
    image: Arc<RgbaImage>,
}

impl fmt::Debug for ImagePreview {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImagePreview")
            .field("size", &self.size())
            .finish()
    }
}

impl ImagePreview {
    /// Return the size of the preview in pixels.
    pub fn size(&self) -> Size {
        Size::new(self.image.width(), self.image.height())
    }

    /// Return the bytes that the pixels hold.
    pub fn byte_len(&self) -> usize {
        self.image.as_raw().len()
    }
}

/// Downsample with premultiplied colors, so a transparent pixel's color does
/// not bleed into its neighbours, and keep the alpha.
fn thumbnail(image: DynamicImage, bounds: Size) -> RgbaImage {
    if image.width() <= bounds.w && image.height() <= bounds.h {
        return image.into_rgba8();
    }
    if !image.has_alpha() {
        return image.thumbnail(bounds.w, bounds.h).into_rgba8();
    }
    let mut rgba = image.into_rgba8();
    for pixel in rgba.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            *channel = (u16::from(*channel) * alpha / 255) as u8;
        }
    }
    let mut reduced = DynamicImage::ImageRgba8(rgba)
        .thumbnail(bounds.w, bounds.h)
        .into_rgba8();
    for pixel in reduced.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            if let Some(value) = (u16::from(*channel) * 255).checked_div(alpha) {
                *channel = value.min(255) as u8;
            }
        }
    }
    reduced
}

/// Match `image` to a box of `size` cells in `tier`, fitted and centred, over
/// a `ground` that shows through its transparent pixels and around it. Cells
/// come row by row, as the image view draws them.
pub fn image_cells(
    image: &RgbaImage,
    size: Size,
    ground: [u8; 3],
    tier: Tier,
) -> Vec<Cell<[u8; 3]>> {
    let (cols, rows) = (size.w as usize, size.h as usize);
    let (width, height) = (image.width(), image.height());
    let mut cells = Vec::new();
    if cols == 0 || rows == 0 || width == 0 || height == 0 {
        return cells;
    }
    // A cell is one display subpixel wide and two tall, as in the view.
    let zoom = (size.w as f32 / width as f32).min(2.0 * size.h as f32 / height as f32);
    let offset = (
        (size.w as f32 - width as f32 * zoom) / 2.0,
        (2.0 * size.h as f32 - height as f32 * zoom) / 2.0,
    );
    let placement = Placement {
        origin: [-offset.0 / zoom, -offset.1 / zoom],
        step: [1.0 / (8.0 * zoom), 1.0 / (4.0 * zoom)],
    };
    let scratch = (&mut LinearRaster::new(), &mut Vec::new());
    let grounds = vec![ground; cols * rows];
    match_cells(
        image,
        placement,
        (cols, rows),
        &grounds,
        tier,
        scratch,
        &mut cells,
    );
    cells
}

/// Match `image`, sampled at `placement`, to a grid of `cols` by `rows`
/// cells in `tier` over one ground each, through reusable scratch rasters.
/// Cells come row by row.
fn match_cells(
    image: &RgbaImage,
    placement: Placement,
    (cols, rows): (usize, usize),
    grounds: &[[u8; 3]],
    tier: Tier,
    (linear, opaque): (&mut LinearRaster, &mut Vec<[u8; 3]>),
    cells: &mut Vec<Cell<[u8; 3]>>,
) {
    let (width, height) = (image.width() as usize, image.height() as usize);
    let Ok(source) = Image::new(image.as_raw(), width, height) else {
        return;
    };
    resample(&source, placement, cols * MATRIX, rows * MATRIX, linear);
    composite(linear, grounds, opaque);
    let stride = cols * MATRIX;
    if let Ok(raster) = RasterView::new(opaque, stride, rows * MATRIX, stride) {
        rgb_into(&raster, SymbolSet::tier(tier), cells);
    }
}

/// What a cached mosaic depends on, besides the ground of each cell.
#[derive(Debug, Clone, Copy, PartialEq)]
struct MosaicKey {
    /// The visible view, in canvas coordinates.
    view: Rect,
    /// Zoom in display subpixels per image pixel.
    zoom: f32,
    /// Display subpixel offset that centers the image.
    offset: (f32, f32),
    /// The mosaic tier.
    tier: Tier,
}

/// The cells of the last render, and what they depend on.
struct Mosaic {
    /// The view, zoom, offset, and tier of the cells.
    key: MosaicKey,
    /// The ground color under each cell, row by row, without effects.
    grounds: Vec<[u8; 3]>,
    /// The matched cells, row by row.
    cells: Vec<Cell<[u8; 3]>>,
}

/// How an image view sizes its image.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Zoom {
    /// Fit the whole image to the view, whatever its size.
    Fit,
    /// Hold a zoom factor in display subpixels per image pixel. A display
    /// subpixel is one column by half a row.
    Manual(f32),
}

/// Widget that renders an image into terminal cells.
///
/// Each cell is an 8×8 matrix of the image, matched to the block mosaic
/// symbol and the two colors that reproduce it best. The tier of symbols
/// comes from the terminal's glyphs, unless the view has a tier of its own.
pub struct ImageView {
    /// The image, RGBA8 with unassociated alpha, shared with the preview it
    /// came from.
    image: Arc<RgbaImage>,
    /// Whether the view fits the image or holds a zoom.
    zoom: Zoom,
    /// Whether the view takes focus, which its zoom and pan keys need.
    focusable: bool,
    /// The view's own mosaic tier, or none to follow the terminal's glyphs.
    tier: Option<Tier>,
    /// The cells of the last render.
    mosaic: Option<Mosaic>,
    /// Scratch raster in linear light, kept between renders.
    linear: LinearRaster,
    /// Scratch opaque raster, kept between renders.
    opaque: Vec<[u8; 3]>,
    /// Scratch ground colors of the render under way, kept between renders.
    grounds: Vec<[u8; 3]>,
    /// Matches since the view was made, which tests count.
    matches: usize,
}

#[derive_commands]
impl ImageView {
    /// Convert the cached image width to a float.
    fn image_width_f32(&self) -> f32 {
        self.image.width() as f32
    }

    /// Convert the cached image height to a float.
    fn image_height_f32(&self) -> f32 {
        self.image.height() as f32
    }

    /// Convert the view width to display subpixels.
    fn view_subpixel_width(view_size: Size) -> f32 {
        view_size.w as f32
    }

    /// Convert the view height to display subpixels.
    fn view_subpixel_height(view_size: Size) -> f32 {
        view_size.h as f32 * 2.0
    }

    /// Compute a zoom value that fits the entire image inside the view.
    fn fit_zoom(&self, view_size: Size) -> f32 {
        let image_width = self.image_width_f32();
        let image_height = self.image_height_f32();
        if image_width == 0.0 || image_height == 0.0 || view_size.w == 0 || view_size.h == 0 {
            return 1.0;
        }

        let view_width = Self::view_subpixel_width(view_size);
        let view_height = Self::view_subpixel_height(view_size);
        let zoom_width = view_width / image_width;
        let zoom_height = view_height / image_height;

        zoom_width.min(zoom_height).clamp(0.0, MAX_ZOOM)
    }

    /// Determine the zoom value to use for the provided view.
    fn effective_zoom(&self, view_size: Size) -> f32 {
        match self.zoom {
            Zoom::Fit => self.fit_zoom(view_size),
            Zoom::Manual(zoom) => zoom,
        }
    }

    /// Leave fit for a held zoom, starting from the zoom that fits `view`,
    /// and return the held zoom. Every manual operation starts here.
    fn take_manual(&mut self, view: Size) -> f32 {
        let zoom = self.effective_zoom(view);
        self.zoom = Zoom::Manual(zoom);
        zoom
    }

    /// Zoom around the center of the current view.
    fn zoom_by(&mut self, view_size: Size, scroll: Point, factor: f32) -> Point {
        let zoom_before = self.take_manual(view_size);
        let view_width = Self::view_subpixel_width(view_size);
        let view_height = Self::view_subpixel_height(view_size);
        if view_width == 0.0 || view_height == 0.0 {
            return scroll;
        }

        let (offset_x, offset_y) = self.center_offset(view_size, zoom_before);
        let center_sub_x = scroll.x as f32 - offset_x + view_width / 2.0;
        let center_sub_y = scroll.y as f32 * 2.0 - offset_y + view_height / 2.0;
        let center_image_x = center_sub_x / zoom_before;
        let center_image_y = center_sub_y / zoom_before;

        let min_zoom = MIN_ZOOM.min(self.fit_zoom(view_size));
        let zoom = (zoom_before * factor).clamp(min_zoom, MAX_ZOOM);
        self.zoom = Zoom::Manual(zoom);
        let (new_offset_x, new_offset_y) = self.center_offset(view_size, zoom);
        let new_center_sub_x = center_image_x * zoom;
        let new_center_sub_y = center_image_y * zoom;
        let new_scroll_x = new_center_sub_x + new_offset_x - view_width / 2.0;
        let new_scroll_y = (new_center_sub_y + new_offset_y - view_height / 2.0) / 2.0;
        Point {
            x: new_scroll_x.max(0.0).round() as u32,
            y: new_scroll_y.max(0.0).round() as u32,
        }
    }

    /// Compute the display subpixel offset to center the image in the view.
    fn center_offset(&self, view_size: Size, zoom: f32) -> (f32, f32) {
        let view_width = Self::view_subpixel_width(view_size);
        let view_height = Self::view_subpixel_height(view_size);
        let image_width = self.image_width_f32() * zoom;
        let image_height = self.image_height_f32() * zoom;

        let offset_x = (view_width - image_width).max(0.0) / 2.0;
        let offset_y = (view_height - image_height).max(0.0) / 2.0;

        (offset_x, offset_y)
    }

    /// Match the visible cells, unless the last render matched the same
    /// view, zoom, offset, tier, and the grounds in `self.grounds`. A render
    /// that matches nothing allocates nothing.
    fn refresh_mosaic(&mut self, key: MosaicKey) {
        if let Some(mosaic) = &self.mosaic
            && mosaic.key == key
            && mosaic.grounds == self.grounds
        {
            return;
        }
        let (mut cells, spare) = self
            .mosaic
            .take()
            .map(|mosaic| (mosaic.cells, mosaic.grounds))
            .unwrap_or_default();
        let (cols, rows) = (key.view.w as usize, key.view.h as usize);
        let (offset_x, offset_y) = key.offset;
        // Matrix pixels are an eighth of a subpixel wide, and a quarter of
        // one tall, since a subpixel is half a row.
        let placement = Placement {
            origin: [
                (key.view.tl.x as f32 - offset_x) / key.zoom,
                (key.view.tl.y as f32 * 2.0 - offset_y) / key.zoom,
            ],
            step: [1.0 / (8.0 * key.zoom), 1.0 / (4.0 * key.zoom)],
        };
        cells.clear();
        match_cells(
            &self.image,
            placement,
            (cols, rows),
            &self.grounds,
            key.tier,
            (&mut self.linear, &mut self.opaque),
            &mut cells,
        );
        self.matches += 1;
        // The grounds of this render move into the mosaic, and the buffer of
        // the last one becomes the scratch of the next.
        let grounds = mem::replace(&mut self.grounds, spare);
        self.mosaic = Some(Mosaic {
            key,
            grounds,
            cells,
        });
    }

    /// Render the image into the provided view rectangle. Each cell lays
    /// the image over the ground of the `background` style, without
    /// effects, so a transparent image shows the ground of whatever holds
    /// it. The effect stack then acts on both colors of each cell alike.
    fn render_cells(
        &mut self,
        render: &mut Render,
        view: Rect,
        origin: Point,
        offset: (f32, f32),
        zoom: f32,
    ) -> Result<()> {
        if zoom <= 0.0 || !zoom.is_finite() {
            return Ok(());
        }
        let bounds = Rect::new(origin.x, origin.y, view.w, view.h);
        let point = |col: u32, row: u32| Point {
            x: origin.x + col,
            y: origin.y + row,
        };
        let ground = render.resolve_base_style("background");
        self.grounds.clear();
        for row in 0..view.h {
            for col in 0..view.w {
                let (r, g, b) = ground.resolve_at(bounds, point(col, row)).bg.rgb();
                self.grounds.push([r, g, b]);
            }
        }
        let tier = self.tier.unwrap_or_else(|| tier_for(render.glyphs()));
        let key = MosaicKey {
            view,
            zoom,
            offset,
            tier,
        };
        self.refresh_mosaic(key);
        let Some(mosaic) = &self.mosaic else {
            return Ok(());
        };
        for (index, cell) in mosaic.cells.iter().enumerate() {
            let at = point(index as u32 % view.w, index as u32 / view.w);
            mosaic::put_cell(render, bounds, at, cell.fg, cell.bg, cell.ch)?;
        }
        Ok(())
    }

    /// Create a new image view widget.
    pub fn new(image: &RgbaImage) -> Self {
        Self::shared(Arc::new(image.clone()))
    }

    /// Create an image view of pixels that a preview may share.
    fn shared(image: Arc<RgbaImage>) -> Self {
        Self {
            image,
            zoom: Zoom::Fit,
            focusable: true,
            tier: None,
            mosaic: None,
            linear: LinearRaster::new(),
            opaque: Vec::new(),
            grounds: Vec::new(),
            matches: 0,
        }
    }

    /// Set whether the view takes focus, which its zoom and pan keys need.
    /// A decorative image, such as a logo, stays out of keyboard traversal.
    #[must_use]
    pub fn with_focusable(mut self, focusable: bool) -> Self {
        self.focusable = focusable;
        self
    }

    /// Draw with the symbols of `tier`, whatever glyphs the terminal has.
    #[must_use]
    pub fn with_tier(mut self, tier: Tier) -> Self {
        self.tier = Some(tier);
        self
    }

    /// Draw with the symbols of `tier`, or with none, the symbols that the
    /// terminal's glyphs allow.
    pub fn set_tier(&mut self, tier: Option<Tier>) {
        self.tier = tier;
    }

    /// The view's own mosaic tier, if it has one.
    pub fn tier(&self) -> Option<Tier> {
        self.tier
    }

    /// Create a new image view widget from a file path.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self::new(&read_rgba(path.as_ref())?))
    }

    /// Return the size of the image in pixels.
    pub fn image_size(&self) -> Size {
        Size::new(self.image.width(), self.image.height())
    }

    /// Create a new image view widget from encoded image bytes, such as a
    /// PNG that an application embeds. The format comes from the bytes, and
    /// markup in no raster format parses as an SVG drawing, at its own size.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if image::guess_format(bytes).is_err() && bytes.trim_ascii_start().starts_with(b"<") {
            return Ok(Self::new(&full_drawing(&svg::parse(bytes)?)?));
        }
        let image = image::load_from_memory(bytes).map_err(image_error)?;
        Ok(Self::new(&image.into_rgba8()))
    }

    /// Create an image view with nothing to show yet.
    ///
    /// The view holds a single transparent pixel until [`Self::set_image`] or
    /// [`Self::set_path`] gives it real content. Callers that mount a viewer
    /// before they have an image should keep it hidden until then.
    #[must_use]
    pub fn empty() -> Self {
        Self::new(&RgbaImage::new(1, 1))
    }

    /// Show a different image, returning the view to its auto-fitted state.
    /// The view keeps its focus setting and its tier.
    pub fn set_image(&mut self, image: &RgbaImage) {
        self.replace(Arc::new(image.clone()));
    }

    /// Show `preview`, returning the view to its auto-fitted state. The view
    /// shares the preview's pixels rather than copying them, and keeps its
    /// focus setting and its tier.
    pub fn set_preview(&mut self, preview: ImagePreview) {
        self.replace(preview.image);
    }

    /// Show nothing: a single transparent pixel, as [`Self::empty`] does.
    /// The view keeps its focus setting and its tier.
    pub fn clear(&mut self) {
        self.replace(Arc::new(RgbaImage::new(1, 1)));
    }

    /// Show `image` from an auto-fitted state, keeping the focus setting
    /// and the tier.
    fn replace(&mut self, image: Arc<RgbaImage>) {
        let (focusable, tier) = (self.focusable, self.tier);
        *self = Self::shared(image);
        self.focusable = focusable;
        self.tier = tier;
    }

    /// Show the image at `path`, returning the view to its auto-fitted state.
    ///
    /// The view keeps its current image when the file cannot be read.
    pub fn set_path(&mut self, path: impl AsRef<Path>) -> Result<()> {
        self.set_image(&read_rgba(path.as_ref())?);
        Ok(())
    }

    /// Zoom around the view center.
    /// @param dir The zoom direction.
    #[command]
    pub fn zoom(&mut self, ctx: &mut dyn Context, dir: ZoomDirection) -> Result<()> {
        let view = ctx.view();
        let view_size = view.content_size();
        let factor = match dir {
            ZoomDirection::In => ZOOM_STEP,
            ZoomDirection::Out => 1.0 / ZOOM_STEP,
        };
        let scroll = self.zoom_by(view_size, view.scroll, factor);
        ctx.scroll(ScrollOp::To(scroll));
        Ok(())
    }

    /// Fit the entire image to the view and clear its pan offset.
    #[command]
    pub fn fit(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.zoom = Zoom::Fit;
        ctx.scroll(ScrollOp::To(Point::default()));
        Ok(())
    }

    /// Pan by one step in the specified direction.
    /// @param dir The pan direction.
    #[command]
    pub fn pan(&mut self, ctx: &mut dyn Context, dir: ScrollDirection) -> Result<()> {
        self.take_manual(ctx.view().content_size());
        let step = match dir {
            ScrollDirection::Left | ScrollDirection::Right => PAN_STEP_COLUMNS,
            ScrollDirection::Up | ScrollDirection::Down => PAN_STEP_ROWS,
        };
        ctx.scroll(ScrollOp::Lines(dir, step.unsigned_abs()));
        Ok(())
    }

    /// Step the view's own mosaic tier: from the terminal's glyphs, through
    /// each tier from half blocks to the full tier, and back, so that the
    /// tiers can be compared.
    #[command]
    pub fn cycle_tier(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        self.tier = cycle(self.tier);
        Ok(())
    }
}

impl Widget for ImageView {
    /// Fill the available space in the terminal view.
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn canvas(&self, view: Size, _ctx: &CanvasContext) -> Size {
        let view_size = view;
        let zoom = self.effective_zoom(view_size);
        let width = (self.image_width_f32() * zoom).ceil() as u32;
        let height = ((self.image_height_f32() * zoom) / 2.0).ceil() as u32;
        Size::new(width.max(view.w), height.max(view.h))
    }

    /// Render the current image view into the terminal buffer.
    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let view_rect = view.view_rect();
        if view_rect.w == 0 || view_rect.h == 0 {
            return Ok(());
        }

        let view_size = view.content_size();
        let zoom = self.effective_zoom(view_size);
        let offset = self.center_offset(view_size, zoom);
        self.render_cells(render, view_rect, view.content_origin(), offset, zoom)
    }

    /// Accept focus so key bindings apply to this widget, unless the view
    /// is decorative.
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        self.focusable
    }
}

impl Register for ImageView {
    /// Register commands for the image viewer widget.
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Cursor, sync::Arc};

    use base64::{Engine, engine::general_purpose::STANDARD};
    use canopy::{
        Canopy, CanopyBuilder, ContextExt,
        render::{Glyphs, TermBuf},
        style::{
            AttrSet, Color, GradientSpec, GradientStop, Paint, Style,
            effects::{self, Effect, StyleEffect},
        },
        testing::harness::Harness,
    };
    use image::Rgba;

    use super::*;

    fn make_view(width: u32, height: u32) -> Size {
        Size::new(width, height)
    }

    /// The alpha-weighted mean color and the mean alpha of a region.
    fn average(image: &RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32) -> ([u8; 3], f32) {
        let mut sums = [0u64; 4];
        for y in y0..y1 {
            for x in x0..x1 {
                let pixel = image.get_pixel(x, y);
                let alpha = u64::from(pixel[3]);
                for ch in 0..3 {
                    sums[ch] += u64::from(pixel[ch]) * alpha;
                }
                sums[3] += alpha;
            }
        }
        let area = u64::from((x1 - x0) * (y1 - y0));
        let color = if sums[3] == 0 {
            [0; 3]
        } else {
            [0, 1, 2].map(|ch| (sums[ch] / sums[3]) as u8)
        };
        (color, sums[3] as f32 / (area * 255) as f32)
    }

    /// One pixel laid over a ground, as the view composites it.
    fn over(pixel: [u8; 4], ground: [u8; 3]) -> Color {
        let image = Image::new(&pixel, 1, 1).expect("image");
        let mut linear = LinearRaster::new();
        let placement = Placement {
            origin: [0.0, 0.0],
            step: [1.0 / 8.0, 1.0 / 8.0],
        };
        resample(&image, placement, 8, 8, &mut linear);
        let mut out = Vec::new();
        composite(&linear, &[ground], &mut out);
        let [r, g, b] = out[0];
        Color::Rgb { r, g, b }
    }

    /// An application whose root holds an image view, on a ground.
    fn app(view: ImageView, ground: Color, effects: Vec<Effect>) -> Result<Canopy> {
        let mut canopy = CanopyBuilder::new().build()?;
        canopy.style_mut().rules().bg("background", ground).apply();
        canopy.with_root_context(|ctx| {
            ctx.set_layout_override(ctx.node_id(), Layout::fill().into())?;
            let _ = ctx.add_child(ctx.node_id(), view)?;
            for effect in effects {
                ctx.push_effect(ctx.node_id(), effect)?;
            }
            Ok(())
        })?;
        Ok(canopy)
    }

    #[test]
    fn preview_paths_bound_tables_without_changing_full_resolution_loading() -> Result<()> {
        let directory = tempfile::tempdir().unwrap();
        let source = RgbaImage::from_pixel(512, 256, image::Rgba([80, 160, 40, 255]));
        for extension in ["png", "jpg", "gif", "webp"] {
            let path = directory.path().join(format!("image.{extension}"));
            if extension == "jpg" {
                DynamicImage::ImageRgba8(source.clone())
                    .into_rgb8()
                    .save(&path)
                    .unwrap();
            } else {
                source.save(&path).unwrap();
            }
            let mut view = ImageView::empty();
            view.set_preview(fitted(&path, Size::new(64, 64))?);
            assert_eq!(view.image_size(), Size::new(64, 32), "{extension}");
            let ([r, g, b], alpha) = average(&view.image, 0, 0, 64, 32);
            assert!((alpha - 1.0).abs() < 0.01, "{extension}: opaque");
            assert!(
                r.abs_diff(80) <= 2 && g.abs_diff(160) <= 2 && b.abs_diff(40) <= 2,
                "{extension}: {r}, {g}, {b}"
            );
            view.set_path(&path)?;
            assert_eq!(view.image_size(), Size::new(512, 256));
            let full = ImageView::from_path(&path)?;
            assert_eq!(full.image_size(), Size::new(512, 256));
        }
        Ok(())
    }

    /// Decode the image at `path`, and fit it to `bounds`, as a host that
    /// stages a preview does.
    fn fitted(path: &Path, bounds: Size) -> Result<ImagePreview> {
        DecodedImage::read(path)?.fit(bounds)
    }

    /// A drawing of `width` by `height` user units: a green left half on a
    /// transparent ground.
    fn drawing(width: u32, height: u32) -> String {
        let half = width / 2;
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}">
                <rect width="{half}" height="{height}" fill="#50a028"/>
            </svg>"##
        )
    }

    /// Check that the left half of `image` is green and the right half clear.
    fn assert_drawn(image: &RgbaImage) {
        let (width, height) = image.dimensions();
        let (color, alpha) = average(image, 0, 0, width / 2, height);
        assert!((alpha - 1.0).abs() < 0.01, "left half opaque: {alpha}");
        let green = [0x50, 0xa0, 0x28];
        assert!(
            color
                .iter()
                .zip(green)
                .all(|(have, want)| have.abs_diff(want) <= 2),
            "{color:?}"
        );
        let (_, alpha) = average(image, width / 2 + 1, 0, width, height);
        assert!(alpha < 0.01, "right half clear: {alpha}");
    }

    #[test]
    fn drawings_keep_their_size_in_a_preview_and_grow_to_fill_a_view() -> Result<()> {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("small.SVG");
        fs::write(&path, drawing(40, 20)).unwrap();
        let kept = DecodedImage::read(&path)?.scale(Size::new(64, 64))?;
        assert_eq!(kept.size(), Size::new(40, 20));
        assert_drawn(&kept.image);
        let filled = DecodedImage::read(&path)?.fit(Size::new(64, 64))?;
        assert_eq!(filled.size(), Size::new(64, 32));
        assert_drawn(&filled.image);
        assert_eq!(
            read_preview(&path, Size::new(64, 64))?.dimensions(),
            (40, 20)
        );

        let mut view = ImageView::empty();
        view.set_preview(fitted(&path, Size::new(64, 64))?);
        assert_eq!(view.image_size(), Size::new(64, 32));
        view.set_path(&path)?;
        assert_eq!(view.image_size(), Size::new(40, 20));
        assert_drawn(&view.image);

        let large = directory.path().join("large.svg");
        fs::write(&large, drawing(20_000, 10_000)).unwrap();
        let shrunk = DecodedImage::read(&large)?.scale(Size::new(4096, 4096))?;
        assert_eq!(shrunk.size(), Size::new(1024, 512), "edges stay capped");
        let full = ImageView::from_path(&large)?;
        assert_eq!(full.image_size(), Size::new(4096, 2048));
        Ok(())
    }

    #[test]
    fn drawings_load_from_bytes_and_embedded_images_but_not_linked_files() -> Result<()> {
        let view = ImageView::from_bytes(drawing(40, 20).as_bytes())?;
        assert_eq!(view.image_size(), Size::new(40, 20));
        assert_drawn(&view.image);

        let directory = tempfile::tempdir().unwrap();
        let red = RgbaImage::from_pixel(4, 4, Rgba([255, 0, 0, 255]));
        red.save(directory.path().join("red.png")).unwrap();
        let mut png = Vec::new();
        red.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let encoded = STANDARD.encode(&png);
        let path = directory.path().join("images.svg");
        fs::write(
            &path,
            format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="4">
                    <image width="4" height="4" href="data:image/png;base64,{encoded}"/>
                    <image x="4" width="4" height="4" href="red.png"/>
                </svg>"#
            ),
        )
        .unwrap();
        let image = read_preview(&path, Size::new(8, 8))?;
        let ([r, _, _], alpha) = average(&image, 0, 0, 4, 4);
        assert!(r > 250 && alpha > 0.99, "the embedded image draws");
        let (_, alpha) = average(&image, 4, 0, 8, 4);
        assert!(alpha < 0.01, "the linked file is never read");

        let broken = directory.path().join("broken.svg");
        fs::write(&broken, "<svg").unwrap();
        let error = DecodedImage::read(&broken)
            .err()
            .expect("a broken drawing fails");
        assert!(error.to_string().contains("svg error"), "{error}");
        let error = ImageView::from_bytes(b"<svg")
            .err()
            .expect("broken markup fails");
        assert!(error.to_string().contains("svg error"), "{error}");
        Ok(())
    }

    #[test]
    fn drawings_stay_within_their_source_and_pixel_limits() -> Result<()> {
        let directory = tempfile::tempdir().unwrap();
        // Each use of an embedded image counts: eight of 2048 * 2048 pixels
        // fill the budget, and the ninth does not draw.
        let red = RgbaImage::from_pixel(2048, 2048, Rgba([255, 0, 0, 255]));
        let mut png = Vec::new();
        red.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let uses: String = (0..9)
            .map(|i| format!(r##"<use href="#red" x="{}"/>"##, i * 4))
            .collect();
        let path = directory.path().join("uses.svg");
        fs::write(
            &path,
            format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="36" height="4">
                    <defs><image id="red" width="4" height="4"
                        href="data:image/png;base64,{}"/></defs>
                    {uses}
                </svg>"#,
                STANDARD.encode(&png)
            ),
        )
        .unwrap();
        let image = read_preview(&path, Size::new(36, 4))?;
        let (_, alpha) = average(&image, 28, 0, 32, 4);
        assert!(alpha > 0.99, "the eighth use draws");
        let (_, alpha) = average(&image, 32, 0, 36, 4);
        assert!(alpha < 0.01, "the ninth use is over the budget");

        // Gzipped data, which could expand without bound, does not parse.
        let gzipped = [0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert!(ImageView::from_bytes(&gzipped).is_err());
        let compressed = directory.path().join("compressed.svg");
        fs::write(&compressed, gzipped).unwrap();
        assert!(DecodedImage::read(&compressed).is_err());

        let large = directory.path().join("large.svg");
        let mut source = drawing(4, 4).into_bytes();
        source.resize(16 * 1024 * 1024 + 1, b' ');
        fs::write(&large, source).unwrap();
        let error = DecodedImage::read(&large)
            .err()
            .expect("a large source fails");
        assert!(error.to_string().contains("limited"), "{error}");
        Ok(())
    }

    #[test]
    fn thumbnails_preserve_small_images_and_keep_alpha_while_averaging() {
        let source = RgbaImage::from_fn(4, 2, |x, _| match x {
            0 => image::Rgba([255, 0, 0, 255]),
            1 => image::Rgba([0, 0, 255, 0]),
            _ => image::Rgba([200, 100, 40, 128]),
        });
        let unchanged = thumbnail(DynamicImage::ImageRgba8(source.clone()), Size::new(8, 8));
        assert_eq!(
            unchanged, source,
            "small images must not be resized or recomposited"
        );
        let reduced = thumbnail(DynamicImage::ImageRgba8(source.clone()), Size::new(2, 1));
        assert_eq!(reduced.dimensions(), (2, 1));
        assert_eq!(
            reduced.get_pixel(0, 0)[2],
            0,
            "transparent blue must not bleed into red"
        );
        for x in 0..2 {
            let (expected, expected_alpha) = average(&source, x * 2, 0, x * 2 + 2, 2);
            let (actual, actual_alpha) = average(&reduced, x, 0, x + 1, 1);
            for ch in 0..3 {
                assert!(actual[ch].abs_diff(expected[ch]) <= 2);
            }
            assert!((actual_alpha - expected_alpha).abs() < 0.01);
        }
    }

    #[test]
    fn preview_bounds_preserve_portrait_and_thin_image_aspect_ratios() {
        for (width, height, expected) in [(32, 96, (8, 24)), (4096, 1, (24, 1)), (1, 4096, (1, 24))]
        {
            let source = DynamicImage::ImageRgba8(RgbaImage::new(width, height));
            assert_eq!(thumbnail(source, Size::new(24, 24)).dimensions(), expected);
        }
    }

    #[test]
    fn a_preview_that_fails_to_stage_leaves_the_image_and_zoom() -> Result<()> {
        let directory = tempfile::tempdir().unwrap();
        let valid = directory.path().join("valid.png");
        RgbaImage::new(4, 2).save(&valid).unwrap();
        let invalid = directory.path().join("invalid.png");
        fs::write(&invalid, b"not an image").unwrap();
        let mut view = ImageView::new(&RgbaImage::from_pixel(3, 2, image::Rgba([255, 0, 0, 255])));
        view.zoom = Zoom::Manual(3.0);
        for (path, bounds) in [
            (invalid, Size::new(16, 16)),
            (directory.path().join("missing.png"), Size::new(16, 16)),
            (valid.clone(), Size::new(0, 16)),
            (valid.clone(), Size::new(16, 0)),
        ] {
            assert!(fitted(&path, bounds).is_err());
            assert_eq!(view.image_size(), Size::new(3, 2));
            assert_eq!(view.image.get_pixel(0, 0), &image::Rgba([255, 0, 0, 255]));
            assert_eq!(view.zoom, Zoom::Manual(3.0));
        }
        view.set_preview(fitted(&valid, Size::new(16, 16))?);
        assert_eq!(view.zoom, Zoom::Fit);
        Ok(())
    }

    #[test]
    fn a_staged_preview_shares_its_pixels_with_the_view() -> Result<()> {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image.png");
        RgbaImage::from_pixel(512, 256, image::Rgba([80, 160, 40, 255]))
            .save(&path)
            .unwrap();
        let preview = DecodedImage::read(&path)?.scale(Size::new(64, 64))?;
        assert_eq!(preview.size(), Size::new(64, 32));
        assert_eq!(preview.byte_len(), 64 * 32 * 4);
        assert_eq!(
            *preview.image,
            read_preview(&path, Size::new(64, 64))?,
            "the stages make what read_preview makes"
        );
        assert!(DecodedImage::read(&path)?.scale(Size::new(0, 8)).is_err());

        let mut view = ImageView::empty()
            .with_focusable(false)
            .with_tier(Tier::Halves);
        view.zoom = Zoom::Manual(2.0);
        view.set_preview(preview.clone());
        assert!(Arc::ptr_eq(&view.image, &preview.image), "no copy");
        assert_eq!(view.zoom, Zoom::Fit, "a new preview fits again");
        assert_eq!(view.image_size(), Size::new(64, 32));
        view.clear();
        assert_eq!(view.image_size(), Size::new(1, 1));
        assert_eq!((view.focusable, view.tier), (false, Some(Tier::Halves)));
        Ok(())
    }

    #[test]
    fn preview_rejects_source_pixels_above_budget_before_building_tables() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("large.png");
        // A valid, compressed grayscale image exceeds the decoded pixel
        // budget while remaining well within the file-size budget in fh.
        image::GrayImage::new(8192, 4097).save(&path).unwrap();
        assert!(fs::metadata(&path).unwrap().len() < 16 * 1024 * 1024);
        let error = fitted(&path, Size::new(256, 256)).unwrap_err();
        assert!(error.to_string().contains("image preview limited to"));
    }

    #[test]
    fn oversized_preview_bounds_still_cap_the_retained_thumbnail() -> Result<()> {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image.png");
        image::GrayImage::new(2048, 1024).save(&path).unwrap();
        let mut view = ImageView::empty();
        view.set_preview(fitted(&path, Size::new(u32::MAX, u32::MAX))?);
        assert_eq!(view.image_size(), Size::new(1024, 512));
        assert_eq!(view.image.as_raw().len(), 1024 * 512 * 4);
        Ok(())
    }

    /// A 2×2 image: an opaque and a half-covered pixel over two opaque ones.
    fn effect_image() -> RgbaImage {
        RgbaImage::from_fn(2, 2, |x, y| {
            if y == 0 {
                Rgba([200, 100, 40, if x == 0 { 255 } else { 128 }])
            } else {
                Rgba([40, 80, 120, 255])
            }
        })
    }

    fn render_effect_image(effects: Vec<Effect>) -> Result<TermBuf> {
        // A black ground under the half-covered pixel.
        let canopy = app(ImageView::new(&effect_image()), Color::Black, effects)?;
        let mut harness = Harness::from_canopy(canopy, Size::new(2, 1))?;
        harness.render()?;
        Ok(harness.buf().clone())
    }

    #[test]
    fn effects_act_on_both_colors_of_each_cell_after_compositing() -> Result<()> {
        let plain = render_effect_image(vec![])?;
        let dimmed = render_effect_image(vec![effects::brightness(0.5), effects::bold()])?;
        let dim = |color: Color| {
            effects::brightness(0.5)
                .apply(Style {
                    fg: color.into(),
                    bg: color.into(),
                    attrs: AttrSet::default(),
                })
                .resolve_at(Rect::new(0, 0, 1, 1), Point::default())
                .fg
        };
        let bottom = Color::Rgb {
            r: 40,
            g: 80,
            b: 120,
        };
        let tops = [
            Color::Rgb {
                r: 200,
                g: 100,
                b: 40,
            },
            over([200, 100, 40, 128], [0, 0, 0]),
        ];
        for (x, top) in tops.into_iter().enumerate() {
            let point = Point { x: x as u32, y: 0 };
            let plain_cell = plain.get(point).expect("plain pixel cell");
            let dimmed_cell = dimmed.get(point).expect("dimmed pixel cell");
            assert_eq!(plain_cell.ch, '▀');
            assert_eq!((plain_cell.style.fg, plain_cell.style.bg), (top, bottom));
            assert_eq!(plain_cell.style.attrs, AttrSet::default());
            assert_eq!(dimmed_cell.ch, '▀');
            assert_eq!(
                (dimmed_cell.style.fg, dimmed_cell.style.bg),
                (dim(top), dim(bottom))
            );
            assert!(dimmed_cell.style.attrs.bold);
        }
        Ok(())
    }

    #[derive(Debug)]
    struct PixelGradient;

    impl StyleEffect for PixelGradient {
        fn apply(&self, mut style: Style) -> Style {
            style.fg = Paint::gradient(GradientSpec::with_stops(
                0.0,
                vec![GradientStop::new(0.0, Color::Blue)],
            ));
            style
        }
    }

    #[test]
    fn an_effect_that_replaces_the_foreground_recolors_the_whole_image() -> Result<()> {
        let buf = render_effect_image(vec![Arc::new(PixelGradient)])?;
        for x in 0..2 {
            let cell = buf.get(Point { x, y: 0 }).expect("pixel cell");
            assert_eq!((cell.style.fg, cell.style.bg), (Color::Blue, Color::Blue));
        }
        Ok(())
    }

    #[test]
    fn fit_zoom_scales_down_below_min_zoom() {
        let image = RgbaImage::new(2000, 1000);
        let view = ImageView::new(&image);
        let zoom = view.fit_zoom(make_view(100, 25));
        assert!(zoom < MIN_ZOOM);
        assert!((zoom - 0.05).abs() < 0.0001);
    }

    #[test]
    fn fit_zoom_scales_up_when_view_is_larger() {
        let image = RgbaImage::new(20, 10);
        let view = ImageView::new(&image);
        let zoom = view.fit_zoom(make_view(100, 25));
        assert!((zoom - 5.0).abs() < 0.0001);
    }

    #[test]
    fn zoom_out_clamps_to_fit_zoom_when_needed() {
        let image = RgbaImage::new(2000, 1000);
        let mut view = ImageView::new(&image);
        let view_size = make_view(100, 25);
        let _ = view.zoom_by(view_size, Point::default(), 0.01);
        let fit_zoom = view.fit_zoom(view_size);
        assert!((view.effective_zoom(view_size) - fit_zoom).abs() < 0.0001);
    }

    #[test]
    fn regression_a_pan_after_a_new_image_keeps_the_fitted_zoom() -> Result<()> {
        let canopy = app(
            ImageView::new(&RgbaImage::new(40, 40)),
            Color::Black,
            vec![],
        )?;
        let mut harness = Harness::from_canopy(canopy, Size::new(20, 10))?;
        harness.render()?;
        harness.with_unique::<ImageView, _>(|view, ctx| {
            let size = ctx.view().content_size();
            assert!((view.effective_zoom(size) - 0.5).abs() < 0.0001);
            // No render runs between the new image and the pan.
            view.set_image(&RgbaImage::new(80, 80));
            view.pan(ctx, ScrollDirection::Down)?;
            let zoom = view.effective_zoom(size);
            assert!(
                (zoom - 0.25).abs() < 0.0001,
                "the pan holds the new image's fitted zoom, not {zoom}"
            );
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn an_image_decodes_from_bytes_with_its_alpha() -> Result<()> {
        let source = RgbaImage::from_fn(2, 1, |x, _| Rgba([10, 20, 30, [0, 255][x as usize]]));
        let mut png = Vec::new();
        DynamicImage::ImageRgba8(source.clone())
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let view = ImageView::from_bytes(&png)?;
        assert_eq!(view.image_size(), Size::new(2, 1));
        assert_eq!(view.image.get_pixel(0, 0)[3], 0);
        assert_eq!(view.image.get_pixel(1, 0), source.get_pixel(1, 0));
        assert!(ImageView::from_bytes(b"not an image").is_err());
        Ok(())
    }

    #[test]
    fn the_view_around_the_image_shows_the_ground() -> Result<()> {
        // A 4×4 image fits 4 columns by 2 rows, centered in 8 by 2.
        let image = RgbaImage::from_pixel(4, 4, Rgba([255, 0, 0, 255]));
        let ground = Color::Rgb { r: 0, g: 0, b: 200 };
        let mut harness = Harness::from_canopy(
            app(ImageView::new(&image), ground, vec![])?,
            Size::new(8, 2),
        )?;
        harness.render()?;
        let cell = |x| harness.buf().get(Point { x, y: 0 }).expect("cell").style;
        assert_eq!((cell(0).fg, cell(0).bg), (ground, ground));
        assert_eq!(cell(3).bg, Color::Rgb { r: 255, g: 0, b: 0 });
        Ok(())
    }

    #[test]
    fn transparent_pixels_show_the_ground_of_the_view() -> Result<()> {
        // A clear pixel, a half-covered one, and an opaque one.
        let image =
            RgbaImage::from_fn(3, 2, |x, _| Rgba([200, 100, 40, [0, 128, 255][x as usize]]));
        let ground = Color::Rgb { r: 0, g: 0, b: 200 };
        let mut harness = Harness::from_canopy(
            app(ImageView::new(&image), ground, vec![])?,
            Size::new(3, 1),
        )?;
        harness.render()?;
        let cell = |x| harness.buf().get(Point { x, y: 0 }).expect("cell").style;
        assert_eq!((cell(0).fg, cell(0).bg), (ground, ground));
        assert_eq!(cell(1).bg, over([200, 100, 40, 128], [0, 0, 200]));
        assert_eq!(
            cell(2).bg,
            Color::Rgb {
                r: 200,
                g: 100,
                b: 40
            }
        );
        Ok(())
    }

    /// A diagonal edge: dark below the line from the top-left corner to the
    /// bottom-right one.
    fn diagonal() -> RgbaImage {
        RgbaImage::from_fn(64, 64, |x, y| {
            if y > x {
                Rgba([20, 20, 20, 255])
            } else {
                Rgba([230, 230, 230, 255])
            }
        })
    }

    /// The characters a view of `diagonal` draws in a tier.
    fn drawn(tier: Option<Tier>, glyphs: Glyphs) -> Result<Vec<char>> {
        let mut view = ImageView::new(&diagonal());
        view.set_tier(tier);
        let mut canopy = app(view, Color::Black, vec![])?;
        canopy.set_glyphs(glyphs);
        let mut harness = Harness::from_canopy(canopy, Size::new(16, 8))?;
        harness.render()?;
        let buf = harness.buf();
        Ok((0..8)
            .flat_map(|y| (0..16).map(move |x| Point { x, y }))
            .filter_map(|point| buf.get(point).map(|cell| cell.ch))
            .collect())
    }

    #[test]
    fn each_tier_draws_with_its_own_symbols() -> Result<()> {
        let halves = drawn(Some(Tier::Halves), Glyphs::Blocks)?;
        assert!(
            halves.iter().all(|ch| matches!(ch, ' ' | '▀')),
            "{halves:?}"
        );
        let blocks = drawn(None, Glyphs::Blocks)?;
        assert!(
            blocks
                .iter()
                .all(|ch| ('\u{2580}'..='\u{259F}').contains(ch) || *ch == ' '),
            "{blocks:?}"
        );
        let full = drawn(None, Glyphs::Full)?;
        assert!(
            full.iter().any(|ch| u32::from(*ch) >= 0x1CD00),
            "a diagonal edge takes the mosaics of the full tier: {full:?}"
        );
        Ok(())
    }

    /// The number of matches the view in a harness has run.
    fn matches(harness: &mut Harness) -> usize {
        harness
            .with_unique::<ImageView, _>(|view, _| Ok(view.matches))
            .expect("image view")
    }

    #[test]
    fn a_render_with_nothing_changed_matches_nothing() -> Result<()> {
        let canopy = app(ImageView::new(&diagonal()), Color::Black, vec![])?;
        let mut harness = Harness::from_canopy(canopy, Size::new(16, 8))?;
        harness.render()?;
        assert_eq!(matches(&mut harness), 1);
        harness.render()?;
        assert_eq!(matches(&mut harness), 1);
        // A dim acts after the cache, so it matches nothing either.
        harness.canopy.with_root_context(|ctx| {
            ctx.push_effect(ctx.node_id(), effects::brightness(0.5))?;
            Ok(())
        })?;
        harness.render()?;
        assert_eq!(matches(&mut harness), 1);
        // A new ground lays the image over it, so the view matches again.
        harness
            .canopy
            .style_mut()
            .rules()
            .bg("background", Color::Rgb { r: 0, g: 90, b: 0 })
            .apply();
        harness.render()?;
        assert_eq!(matches(&mut harness), 2);
        Ok(())
    }
}
