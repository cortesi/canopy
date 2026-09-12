//! Image viewer widget with zoom and pan controls.

use std::path::Path;

use canopy::{
    Canopy, CommandEnum, Context, FocusDirection, Loader, Render, ViewContext, Widget,
    derive_commands,
    error::{Error, Result},
    geom::{Point, Rect, Size},
    layout::{CanvasContext, Layout},
    style::{AttrSet, Color, Style},
};
use image::{DynamicImage, ImageDecoder, RgbaImage};

/// Direction for zoom commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, CommandEnum)]
pub enum ZoomDirection {
    /// Zoom in.
    In,
    /// Zoom out.
    Out,
}

/// Character used to render two vertical pixels per terminal cell.
const HALF_BLOCK: char = '\u{2580}';
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

/// Summed-area table for fast image region sampling.
struct IntegralImage {
    /// Row stride in the summed-area tables.
    stride: usize,
    /// Summed red channel values (premultiplied by alpha).
    red: Vec<u64>,
    /// Summed green channel values (premultiplied by alpha).
    green: Vec<u64>,
    /// Summed blue channel values (premultiplied by alpha).
    blue: Vec<u64>,
}

impl IntegralImage {
    /// Build an integral image from an RGBA buffer.
    fn new(image: &RgbaImage) -> Self {
        let width = image.width();
        let height = image.height();
        let stride = (width + 1) as usize;
        let size = stride * (height + 1) as usize;
        let mut red = vec![0u64; size];
        let mut green = vec![0u64; size];
        let mut blue = vec![0u64; size];

        for y in 0..height {
            let mut row_red = 0u64;
            let mut row_green = 0u64;
            let mut row_blue = 0u64;
            for x in 0..width {
                let pixel = image.get_pixel(x, y);
                let alpha = pixel[3] as u64;
                row_red += (pixel[0] as u64 * alpha) / 255;
                row_green += (pixel[1] as u64 * alpha) / 255;
                row_blue += (pixel[2] as u64 * alpha) / 255;

                let idx = (y as usize + 1) * stride + (x as usize + 1);
                let above = idx - stride;
                red[idx] = red[above] + row_red;
                green[idx] = green[above] + row_green;
                blue[idx] = blue[above] + row_blue;
            }
        }

        Self {
            stride,
            red,
            green,
            blue,
        }
    }

    /// Sum a channel over a rectangular region (exclusive end coordinates).
    fn sum_channel(&self, channel: &[u64], left: u32, top: u32, right: u32, bottom: u32) -> u64 {
        let left = left as usize;
        let right = right as usize;
        let top = top as usize;
        let bottom = bottom as usize;
        let idx = |x: usize, y: usize| y * self.stride + x;
        let a = channel[idx(right, bottom)];
        let b = channel[idx(left, top)];
        let c = channel[idx(right, top)];
        let d = channel[idx(left, bottom)];
        a + b - c - d
    }

    /// Sum all RGB channels over a region.
    fn sum_rgb(&self, left: u32, top: u32, right: u32, bottom: u32) -> (u64, u64, u64) {
        (
            self.sum_channel(&self.red, left, top, right, bottom),
            self.sum_channel(&self.green, left, top, right, bottom),
            self.sum_channel(&self.blue, left, top, right, bottom),
        )
    }
}

/// Decode an image file into RGBA pixels.
fn read_rgba(path: &Path) -> Result<RgbaImage> {
    let image = image::open(path).map_err(|err| Error::Invalid(format!("image error: {err}")))?;
    Ok(image.into_rgba8())
}

/// Decode within a source budget, then shrink before building sampling tables.
fn read_preview(path: &Path, bounds: Size) -> Result<RgbaImage> {
    if bounds.w == 0 || bounds.h == 0 {
        return Err(Error::Invalid(
            "image preview bounds must be nonzero".into(),
        ));
    }
    let image_error = |err| Error::Invalid(format!("image error: {err}"));
    let mut reader = image::ImageReader::open(path)
        .map_err(|err| Error::Invalid(format!("image error: {err}")))?;
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
    // remaining budget to the decoder. Keep that accounting after inspecting
    // dimensions, so conversion and sampling cannot expand an unchecked image.
    limits.reserve(decoder.total_bytes()).map_err(image_error)?;
    decoder.set_limits(limits).map_err(image_error)?;
    let image = DynamicImage::from_decoder(decoder).map_err(image_error)?;
    Ok(thumbnail(image, bounds))
}

/// Downsample colors already composited onto the viewer's black background.
fn thumbnail(image: DynamicImage, bounds: Size) -> RgbaImage {
    if image.width() <= bounds.w && image.height() <= bounds.h {
        return image.into_rgba8();
    }
    let image = if image.has_alpha() {
        let mut rgba = image.into_rgba8();
        for pixel in rgba.pixels_mut() {
            let alpha = u16::from(pixel[3]);
            if alpha != 255 {
                for channel in &mut pixel.0[..3] {
                    *channel = (u16::from(*channel) * alpha / 255) as u8;
                }
                pixel[3] = 255;
            }
        }
        DynamicImage::ImageRgba8(rgba)
    } else {
        image
    };
    image.thumbnail(bounds.w, bounds.h).into_rgba8()
}

/// Widget that renders an image into terminal cells.
pub struct ImageView {
    /// Cached image width in pixels.
    image_width: u32,
    /// Cached image height in pixels.
    image_height: u32,
    /// Integral image for fast sampling.
    integral: IntegralImage,
    /// Zoom factor in display subpixels per image pixel.
    zoom: f32,
    /// Whether the view should auto-fit the image to the terminal.
    auto_fit: bool,
}

#[derive_commands]
impl ImageView {
    /// Convert the cached image width to a float.
    fn image_width_f32(&self) -> f32 {
        self.image_width as f32
    }

    /// Convert the cached image height to a float.
    fn image_height_f32(&self) -> f32 {
        self.image_height as f32
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
        if self.auto_fit {
            self.fit_zoom(view_size)
        } else {
            self.zoom
        }
    }

    /// Apply automatic fit if enabled.
    fn apply_auto_fit(&mut self, view_size: Size) {
        if !self.auto_fit {
            return;
        }
        if view_size.w == 0 || view_size.h == 0 {
            return;
        }

        self.zoom = self.fit_zoom(view_size);
    }

    /// Zoom around the center of the current view.
    fn zoom_by(&mut self, view_size: Size, scroll: Point, factor: f32) -> Point {
        let view_width = Self::view_subpixel_width(view_size);
        let view_height = Self::view_subpixel_height(view_size);
        if view_width == 0.0 || view_height == 0.0 {
            return scroll;
        }

        let zoom_before = self.zoom;
        let (offset_x, offset_y) = self.center_offset(view_size, zoom_before);
        let center_sub_x = scroll.x as f32 - offset_x + view_width / 2.0;
        let center_sub_y = scroll.y as f32 * 2.0 - offset_y + view_height / 2.0;
        let center_image_x = center_sub_x / zoom_before;
        let center_image_y = center_sub_y / zoom_before;

        let min_zoom = MIN_ZOOM.min(self.fit_zoom(view_size));
        self.zoom = (self.zoom * factor).clamp(min_zoom, MAX_ZOOM);
        let (new_offset_x, new_offset_y) = self.center_offset(view_size, self.zoom);
        let new_center_sub_x = center_image_x * self.zoom;
        let new_center_sub_y = center_image_y * self.zoom;
        let new_scroll_x = new_center_sub_x + new_offset_x - view_width / 2.0;
        let new_scroll_y = (new_center_sub_y + new_offset_y - view_height / 2.0) / 2.0;
        Point {
            x: new_scroll_x.max(0.0).round() as u32,
            y: new_scroll_y.max(0.0).round() as u32,
        }
    }

    /// Compute the image-space bounds of a display subpixel.
    fn subpixel_bounds(
        &self,
        zoom: f32,
        subpixel_column: f32,
        subpixel_row: f32,
    ) -> (f32, f32, f32, f32) {
        let inverse_zoom = 1.0 / zoom;
        let left = subpixel_column * inverse_zoom;
        let right = (subpixel_column + 1.0) * inverse_zoom;
        let top = subpixel_row * inverse_zoom;
        let bottom = (subpixel_row + 1.0) * inverse_zoom;
        (left, top, right, bottom)
    }

    /// Sample a color from the image for a display subpixel.
    fn sample_color(&self, zoom: f32, subpixel_column: f32, subpixel_row: f32) -> Color {
        let (left, top, right, bottom) = self.subpixel_bounds(zoom, subpixel_column, subpixel_row);
        let center_column = (left + right) * 0.5;
        let center_row = (top + bottom) * 0.5;
        if center_column < 0.0
            || center_row < 0.0
            || center_column >= self.image_width_f32()
            || center_row >= self.image_height_f32()
        {
            return Color::Black;
        }

        let Some((red, green, blue)) = self.sample_region(left, top, right, bottom) else {
            return Color::Black;
        };

        Color::Rgb {
            r: red,
            g: green,
            b: blue,
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

    /// Sample a rectangular region in image space and return the average color.
    fn sample_region(&self, left: f32, top: f32, right: f32, bottom: f32) -> Option<(u8, u8, u8)> {
        if self.image_width == 0 || self.image_height == 0 {
            return None;
        }

        let left_index = left.floor() as i32;
        let right_index = right.ceil() as i32;
        let top_index = top.floor() as i32;
        let bottom_index = bottom.ceil() as i32;

        let left_clamped = left_index.max(0) as u32;
        let right_clamped = right_index.min(self.image_width as i32).max(0) as u32;
        let top_clamped = top_index.max(0) as u32;
        let bottom_clamped = bottom_index.min(self.image_height as i32).max(0) as u32;

        if left_clamped >= right_clamped || top_clamped >= bottom_clamped {
            return None;
        }

        let area = (right_clamped - left_clamped) as u64 * (bottom_clamped - top_clamped) as u64;
        let (red_total, green_total, blue_total) =
            self.integral
                .sum_rgb(left_clamped, top_clamped, right_clamped, bottom_clamped);

        let red = (red_total / area) as u8;
        let green = (green_total / area) as u8;
        let blue = (blue_total / area) as u8;

        Some((red, green, blue))
    }

    /// Render the image into the provided view rectangle.
    fn render_cells(
        &self,
        render: &mut Render,
        view: Rect,
        origin: Point,
        offset: (f32, f32),
        zoom: f32,
    ) -> Result<()> {
        let (offset_x, offset_y) = offset;
        let bounds = Rect::new(origin.x, origin.y, view.w, view.h);

        for row_index in 0..view.h {
            let top_subpixel_row = view.tl.y.saturating_add(row_index).saturating_mul(2);
            let bottom_subpixel_row = top_subpixel_row.saturating_add(1);
            let top_row = top_subpixel_row as f32 - offset_y;
            let bottom_row = bottom_subpixel_row as f32 - offset_y;

            for column_index in 0..view.w {
                let column = (view.tl.x + column_index) as f32 - offset_x;
                let top_color = self.sample_color(zoom, column, top_row);
                let bottom_color = self.sample_color(zoom, column, bottom_row);
                let style = render.apply_effects(Style {
                    fg: top_color.into(),
                    bg: bottom_color.into(),
                    attrs: AttrSet::default(),
                });
                let point = Point {
                    x: origin.x + column_index,
                    y: origin.y + row_index,
                };
                render.put_cell(style.resolve_at(bounds, point), point, HALF_BLOCK)?;
            }
        }

        Ok(())
    }

    /// Create a new image view widget.
    pub fn new(image: &RgbaImage) -> Self {
        let image_width = image.width();
        let image_height = image.height();
        let integral = IntegralImage::new(image);
        Self {
            image_width,
            image_height,
            integral,
            zoom: 1.0,
            auto_fit: true,
        }
    }

    /// Create a new image view widget from a file path.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self::new(&read_rgba(path.as_ref())?))
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
    pub fn set_image(&mut self, image: &RgbaImage) {
        *self = Self::new(image);
    }

    /// Show the image at `path`, returning the view to its auto-fitted state.
    ///
    /// The view keeps its current image when the file cannot be read.
    pub fn set_path(&mut self, path: impl AsRef<Path>) -> Result<()> {
        self.set_image(&read_rgba(path.as_ref())?);
        Ok(())
    }

    /// Load a thumbnail within `bounds`, preserving its aspect ratio.
    ///
    /// Small images keep their original pixels. Larger images are averaged
    /// after compositing transparency onto black. Zooming uses the thumbnail;
    /// use [`Self::set_path`] to retain full-resolution zooming instead.
    ///
    /// The source is limited to 32 * 1024 * 1024 pixels and the decoder has a
    /// 128 MiB allocation budget. Conversion can allocate one additional RGBA
    /// source buffer. The sampling tables use at most 24 * (bounds.w + 1) *
    /// (bounds.h + 1) bytes. Zero bounds are invalid. On failure, the current
    /// image is preserved. Callers should also bound the encoded file size.
    pub fn set_preview_path(&mut self, path: impl AsRef<Path>, bounds: Size) -> Result<()> {
        self.set_image(&read_preview(path.as_ref(), bounds)?);
        Ok(())
    }

    /// Zoom around the view center.
    /// @param dir The zoom direction.
    #[command]
    pub fn zoom(&mut self, ctx: &mut dyn Context, dir: ZoomDirection) -> Result<()> {
        let view = ctx.view();
        let view_size = view.content_size();
        self.zoom = self.effective_zoom(view_size);
        self.auto_fit = false;
        let factor = match dir {
            ZoomDirection::In => ZOOM_STEP,
            ZoomDirection::Out => 1.0 / ZOOM_STEP,
        };
        let scroll = self.zoom_by(view_size, view.scroll, factor);
        ctx.scroll_to(scroll.x, scroll.y);
        Ok(())
    }

    /// Pan by one step in the specified direction.
    /// @param dir The pan direction.
    #[command]
    pub fn pan(&mut self, ctx: &mut dyn Context, dir: FocusDirection) -> Result<()> {
        self.auto_fit = false;
        match dir {
            FocusDirection::Left | FocusDirection::Prev => {
                ctx.scroll_by(-PAN_STEP_COLUMNS, 0);
            }
            FocusDirection::Right | FocusDirection::Next => {
                ctx.scroll_by(PAN_STEP_COLUMNS, 0);
            }
            FocusDirection::Up => {
                ctx.scroll_by(0, -PAN_STEP_ROWS);
            }
            FocusDirection::Down => {
                ctx.scroll_by(0, PAN_STEP_ROWS);
            }
        }
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
        self.apply_auto_fit(view_size);

        let offset = self.center_offset(view_size, self.zoom);
        self.render_cells(render, view_rect, view.content_origin(), offset, self.zoom)
    }

    /// Accept focus so key bindings apply to this widget.
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }
}

impl Loader for ImageView {
    /// Register commands for the image viewer widget.
    fn load(cnpy: &mut Canopy) -> Result<()> {
        cnpy.add_commands::<Self>()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Arc};

    use canopy::{
        ContextExt,
        style::{
            GradientSpec, GradientStop, Paint,
            effects::{self, Effect, StyleEffect},
        },
        testing::harness::Harness,
    };
    use image::Rgba;

    use super::*;

    fn make_view(width: u32, height: u32) -> Size {
        Size::new(width, height)
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
            view.set_preview_path(&path, Size::new(64, 64))?;
            assert_eq!(
                (view.image_width, view.image_height),
                (64, 32),
                "{extension}"
            );
            assert_eq!(view.integral.red.len(), 65 * 33);
            let (r, g, b) = view.sample_region(0.0, 0.0, 64.0, 32.0).unwrap();
            assert!(
                r.abs_diff(80) <= 2 && g.abs_diff(160) <= 2 && b.abs_diff(40) <= 2,
                "{extension}: {r}, {g}, {b}"
            );
            view.set_path(&path)?;
            assert_eq!((view.image_width, view.image_height), (512, 256));
            let full = ImageView::from_path(&path)?;
            assert_eq!((full.image_width, full.image_height), (512, 256));
        }
        Ok(())
    }

    #[test]
    fn thumbnails_preserve_small_images_and_composite_before_averaging() {
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
        let full = ImageView::new(&source);
        let preview = ImageView::new(&reduced);
        for x in 0..2 {
            let left = (x * 2) as f32;
            let expected = full.sample_region(left, 0.0, left + 2.0, 2.0).unwrap();
            let actual = preview
                .sample_region(x as f32, 0.0, (x + 1) as f32, 1.0)
                .unwrap();
            assert!(actual.0.abs_diff(expected.0) <= 1);
            assert!(actual.1.abs_diff(expected.1) <= 1);
            assert!(actual.2.abs_diff(expected.2) <= 1);
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
    fn failed_preview_load_keeps_the_previous_image_and_zoom() -> Result<()> {
        let directory = tempfile::tempdir().unwrap();
        let valid = directory.path().join("valid.png");
        RgbaImage::new(4, 2).save(&valid).unwrap();
        let invalid = directory.path().join("invalid.png");
        fs::write(&invalid, b"not an image").unwrap();
        let mut view = ImageView::new(&RgbaImage::from_pixel(3, 2, image::Rgba([255, 0, 0, 255])));
        view.auto_fit = false;
        view.zoom = 3.0;
        for (path, bounds) in [
            (invalid, Size::new(16, 16)),
            (directory.path().join("missing.png"), Size::new(16, 16)),
            (valid.clone(), Size::new(0, 16)),
            (valid.clone(), Size::new(16, 0)),
        ] {
            assert!(view.set_preview_path(path, bounds).is_err());
            assert_eq!((view.image_width, view.image_height), (3, 2));
            assert_eq!(
                view.sample_color(1.0, 0.0, 0.0),
                Color::Rgb { r: 255, g: 0, b: 0 }
            );
            assert!(!view.auto_fit);
            assert_eq!(view.zoom, 3.0);
        }
        view.set_preview_path(valid, Size::new(16, 16))?;
        assert!(view.auto_fit);
        assert_eq!(view.zoom, 1.0);
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
        let mut view = ImageView::empty();
        let error = view
            .set_preview_path(&path, Size::new(256, 256))
            .unwrap_err();
        assert!(error.to_string().contains("image preview limited to"));
        assert_eq!((view.image_width, view.image_height), (1, 1));
        assert_eq!(view.integral.red.len(), 4);
    }

    fn render_effect_image(effects: Vec<Effect>) -> Result<canopy::TermBuf> {
        let image = RgbaImage::from_fn(2, 2, |x, y| {
            if y == 0 {
                Rgba([200, 100, 40, if x == 0 { 255 } else { 128 }])
            } else {
                Rgba([40, 80, 120, 255])
            }
        });
        let mut canopy = Canopy::new();
        canopy.with_root_context(|ctx| {
            ctx.set_layout(Layout::fill())?;
            let _ = ctx.add_child(ImageView::new(&image))?;
            for effect in effects {
                ctx.push_effect(ctx.node_id(), effect)?;
            }
            Ok(())
        })?;
        let mut harness = Harness::from_canopy(canopy, Size::new(2, 1))?;
        harness.render()?;
        Ok(harness.buf().clone())
    }

    #[test]
    fn image_cells_inherit_effects_once_on_both_pixel_channels() -> Result<()> {
        let plain = render_effect_image(vec![])?;
        let dimmed = render_effect_image(vec![effects::brightness(0.5), effects::bold()])?;
        for x in 0..2 {
            let point = Point { x, y: 0 };
            let plain_cell = plain.get(point).expect("plain pixel cell");
            let dimmed_cell = dimmed.get(point).expect("dimmed pixel cell");
            assert_eq!(plain_cell.ch, HALF_BLOCK);
            assert_eq!(dimmed_cell.ch, HALF_BLOCK);
            let plain_fg = if x == 0 {
                Color::Rgb {
                    r: 200,
                    g: 100,
                    b: 40,
                }
            } else {
                Color::Rgb {
                    r: 100,
                    g: 50,
                    b: 20,
                }
            };
            let dimmed_fg = if x == 0 {
                Color::Rgb {
                    r: 100,
                    g: 50,
                    b: 20,
                }
            } else {
                Color::Rgb {
                    r: 50,
                    g: 25,
                    b: 10,
                }
            };
            assert_eq!(plain_cell.style.fg, plain_fg);
            assert_eq!(
                plain_cell.style.bg,
                Color::Rgb {
                    r: 40,
                    g: 80,
                    b: 120
                }
            );
            assert_eq!(plain_cell.style.attrs, AttrSet::default());
            assert_eq!(dimmed_cell.style.fg, dimmed_fg);
            assert_eq!(
                dimmed_cell.style.bg,
                Color::Rgb {
                    r: 20,
                    g: 40,
                    b: 60
                }
            );
            assert_eq!(
                dimmed_cell.style.attrs,
                AttrSet {
                    bold: true,
                    ..AttrSet::default()
                }
            );
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
    fn image_effects_can_replace_solid_paint_with_a_gradient() -> Result<()> {
        let buf = render_effect_image(vec![Arc::new(PixelGradient)])?;
        for x in 0..2 {
            let cell = buf.get(Point { x, y: 0 }).expect("pixel cell");
            assert_eq!(cell.ch, HALF_BLOCK);
            assert_eq!(cell.style.fg, Color::Blue);
            assert_eq!(
                cell.style.bg,
                Color::Rgb {
                    r: 40,
                    g: 80,
                    b: 120
                }
            );
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
        assert!((view.zoom - fit_zoom).abs() < 0.0001);
    }

    #[test]
    fn sample_color_returns_black_outside_image() {
        let image = RgbaImage::from_pixel(4, 4, Rgba([255, 0, 0, 255]));
        let view = ImageView::new(&image);
        assert_eq!(view.sample_color(1.0, -1.0, 0.0), Color::Black);
        assert_eq!(
            view.sample_color(1.0, 0.0, 0.0),
            Color::Rgb { r: 255, g: 0, b: 0 }
        );
    }
}
