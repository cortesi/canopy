//! Image viewer widget with zoom and pan controls.

use std::path::Path;

use canopy::{
    Canopy, Context, Loader, ReadContext, Widget, command,
    commands::{ScrollDirection, ZoomDirection},
    derive_commands, error as canopy_error,
    geom::{Expanse, Point, Rect},
    layout::{CanvasContext, Layout, Size},
    render::Render,
    style::{AttrSet, Color, ResolvedStyle},
};
use image::RgbaImage;

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

/// Widget that renders an image into terminal cells.
pub struct ImageView {
    /// Loaded image buffer.
    image: RgbaImage,
    /// Cached image width in pixels.
    image_width: u32,
    /// Cached image height in pixels.
    image_height: u32,
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
    fn view_subpixel_width(view_size: Expanse) -> f32 {
        view_size.w as f32
    }

    /// Convert the view height to display subpixels.
    fn view_subpixel_height(view_size: Expanse) -> f32 {
        view_size.h as f32 * 2.0
    }

    /// Compute a zoom value that fits the entire image inside the view.
    fn fit_zoom(&self, view_size: Expanse) -> f32 {
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
    fn effective_zoom(&self, view_size: Expanse) -> f32 {
        if self.auto_fit {
            self.fit_zoom(view_size)
        } else {
            self.zoom
        }
    }

    /// Apply automatic fit if enabled.
    fn apply_auto_fit(&mut self, view_size: Expanse) {
        if !self.auto_fit {
            return;
        }
        if view_size.w == 0 || view_size.h == 0 {
            return;
        }

        self.zoom = self.fit_zoom(view_size);
    }

    /// Zoom around the center of the current view.
    fn zoom_by(&mut self, view_size: Expanse, scroll: Point, factor: f32) -> Point {
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
    fn center_offset(&self, view_size: Expanse, zoom: f32) -> (f32, f32) {
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

        let left_clamped = left_index.max(0);
        let right_clamped = right_index.min(self.image_width as i32);
        let top_clamped = top_index.max(0);
        let bottom_clamped = bottom_index.min(self.image_height as i32);

        if left_clamped >= right_clamped || top_clamped >= bottom_clamped {
            return None;
        }

        let mut red_total: u32 = 0;
        let mut green_total: u32 = 0;
        let mut blue_total: u32 = 0;
        let mut sample_count: u32 = 0;

        for row_index in top_clamped..bottom_clamped {
            for column_index in left_clamped..right_clamped {
                let pixel = self.image.get_pixel(column_index as u32, row_index as u32);
                let alpha = pixel[3] as u32;
                red_total += (pixel[0] as u32 * alpha) / 255;
                green_total += (pixel[1] as u32 * alpha) / 255;
                blue_total += (pixel[2] as u32 * alpha) / 255;
                sample_count += 1;
            }
        }

        if sample_count == 0 {
            return None;
        }

        let red = (red_total / sample_count) as u8;
        let green = (green_total / sample_count) as u8;
        let blue = (blue_total / sample_count) as u8;

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
    ) -> canopy_error::Result<()> {
        let (offset_x, offset_y) = offset;

        for row_index in 0..view.h {
            let top_subpixel_row = view.tl.y.saturating_add(row_index).saturating_mul(2);
            let bottom_subpixel_row = top_subpixel_row.saturating_add(1);
            let top_row = top_subpixel_row as f32 - offset_y;
            let bottom_row = bottom_subpixel_row as f32 - offset_y;

            for column_index in 0..view.w {
                let column = (view.tl.x + column_index) as f32 - offset_x;
                let top_color = self.sample_color(zoom, column, top_row);
                let bottom_color = self.sample_color(zoom, column, bottom_row);
                let style = ResolvedStyle::new(top_color, bottom_color, AttrSet::default());
                let point = Point {
                    x: origin.x + column_index,
                    y: origin.y + row_index,
                };
                render.put_cell(style, point, HALF_BLOCK)?;
            }
        }

        Ok(())
    }

    /// Create a new image view widget.
    pub fn new(image: RgbaImage) -> Self {
        let image_width = image.width();
        let image_height = image.height();
        Self {
            image,
            image_width,
            image_height,
            zoom: 1.0,
            auto_fit: true,
        }
    }

    /// Create a new image view widget from a file path.
    pub fn from_path(path: impl AsRef<Path>) -> canopy_error::Result<Self> {
        let image = image::open(path.as_ref())
            .map_err(|err| canopy_error::Error::Invalid(format!("image error: {err}")))?;
        Ok(Self::new(image.to_rgba8()))
    }

    /// Configure whether the image auto-fits to the view.
    pub fn with_auto_fit(mut self, auto_fit: bool) -> Self {
        self.auto_fit = auto_fit;
        self
    }

    /// Zoom around the view center.
    pub fn zoom(&mut self, ctx: &mut dyn Context, dir: ZoomDirection) -> canopy_error::Result<()> {
        let view = ctx.view();
        let view_size = view.content_size();
        self.zoom = self.effective_zoom(view_size);
        self.auto_fit = false;
        let factor = match dir {
            ZoomDirection::In => ZOOM_STEP,
            ZoomDirection::Out => 1.0 / ZOOM_STEP,
        };
        let scroll = self.zoom_by(view_size, view.tl, factor);
        ctx.scroll_to(scroll.x, scroll.y);
        Ok(())
    }

    /// Pan by one step in the specified direction.
    pub fn pan(&mut self, ctx: &mut dyn Context, dir: ScrollDirection) -> canopy_error::Result<()> {
        self.auto_fit = false;
        match dir {
            ScrollDirection::Left => {
                ctx.scroll_by(-PAN_STEP_COLUMNS, 0);
            }
            ScrollDirection::Right => {
                ctx.scroll_by(PAN_STEP_COLUMNS, 0);
            }
            ScrollDirection::Up => {
                ctx.scroll_by(0, -PAN_STEP_ROWS);
            }
            ScrollDirection::Down => {
                ctx.scroll_by(0, PAN_STEP_ROWS);
            }
        }
        Ok(())
    }

    #[command]
    /// Zoom in around the view center.
    pub fn zoom_in(&mut self, ctx: &mut dyn Context) -> canopy_error::Result<()> {
        self.zoom(ctx, ZoomDirection::In)
    }

    #[command]
    /// Zoom out around the view center.
    pub fn zoom_out(&mut self, ctx: &mut dyn Context) -> canopy_error::Result<()> {
        self.zoom(ctx, ZoomDirection::Out)
    }

    #[command]
    /// Pan up by one step.
    pub fn pan_up(&mut self, ctx: &mut dyn Context) -> canopy_error::Result<()> {
        self.pan(ctx, ScrollDirection::Up)
    }

    #[command]
    /// Pan down by one step.
    pub fn pan_down(&mut self, ctx: &mut dyn Context) -> canopy_error::Result<()> {
        self.pan(ctx, ScrollDirection::Down)
    }

    #[command]
    /// Pan left by one step.
    pub fn pan_left(&mut self, ctx: &mut dyn Context) -> canopy_error::Result<()> {
        self.pan(ctx, ScrollDirection::Left)
    }

    #[command]
    /// Pan right by one step.
    pub fn pan_right(&mut self, ctx: &mut dyn Context) -> canopy_error::Result<()> {
        self.pan(ctx, ScrollDirection::Right)
    }
}

impl Widget for ImageView {
    /// Fill the available space in the terminal view.
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn canvas(&self, view: Size<u32>, _ctx: &CanvasContext) -> Size<u32> {
        let view_size = Expanse::new(view.width, view.height);
        let zoom = self.effective_zoom(view_size);
        let width = (self.image_width_f32() * zoom).ceil() as u32;
        let height = ((self.image_height_f32() * zoom) / 2.0).ceil() as u32;
        Size::new(width.max(view.width), height.max(view.height))
    }

    /// Render the current image view into the terminal buffer.
    fn render(&mut self, render: &mut Render, ctx: &dyn ReadContext) -> canopy_error::Result<()> {
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
    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        true
    }
}

impl Loader for ImageView {
    /// Register commands for the image viewer widget.
    fn load(cnpy: &mut Canopy) -> canopy_error::Result<()> {
        cnpy.add_commands::<Self>()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use image::Rgba;

    use super::*;

    fn make_view(width: u32, height: u32) -> Expanse {
        Expanse::new(width, height)
    }

    #[test]
    fn fit_zoom_scales_down_below_min_zoom() {
        let image = RgbaImage::new(2000, 1000);
        let view = ImageView::new(image);
        let zoom = view.fit_zoom(make_view(100, 25));
        assert!(zoom < MIN_ZOOM);
        assert!((zoom - 0.05).abs() < 0.0001);
    }

    #[test]
    fn fit_zoom_scales_up_when_view_is_larger() {
        let image = RgbaImage::new(20, 10);
        let view = ImageView::new(image);
        let zoom = view.fit_zoom(make_view(100, 25));
        assert!((zoom - 5.0).abs() < 0.0001);
    }

    #[test]
    fn zoom_out_clamps_to_fit_zoom_when_needed() {
        let image = RgbaImage::new(2000, 1000);
        let mut view = ImageView::new(image);
        let view_size = make_view(100, 25);
        let _ = view.zoom_by(view_size, Point::default(), 0.01);
        let fit_zoom = view.fit_zoom(view_size);
        assert!((view.zoom - fit_zoom).abs() < 0.0001);
    }

    #[test]
    fn sample_color_returns_black_outside_image() {
        let image = RgbaImage::from_pixel(4, 4, Rgba([255, 0, 0, 255]));
        let view = ImageView::new(image);
        assert_eq!(view.sample_color(1.0, -1.0, 0.0), Color::Black);
        assert_eq!(
            view.sample_color(1.0, 0.0, 0.0),
            Color::Rgb { r: 255, g: 0, b: 0 }
        );
    }
}
