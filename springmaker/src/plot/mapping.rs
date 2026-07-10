//! Pure data↔pixel affine mapping for chart hover. Bitmap-space geometry is
//! derived from the SAME constants the renderer hands plotters, so the two
//! cannot drift; the letterbox composes widget↔bitmap coordinates on top.

use super::{CHART_H, CHART_W, MARGIN, X_LABEL_AREA, Y_LABEL_AREA};

#[allow(dead_code)] // consumed from Task 4 (canvas); remove this allow then
pub struct ChartMapping {
    /// Data ranges plotters was given (AFTER the 1.1 headroom factor).
    pub x_max: f64,
    pub y_max: f64,
}

impl ChartMapping {
    /// (x0, y0, x1, y1) of the plot area inside the bitmap, in pixels.
    #[allow(dead_code)] // consumed from Task 4 (canvas); remove this allow then
    pub fn plot_rect() -> (f32, f32, f32, f32) {
        (
            (MARGIN + Y_LABEL_AREA) as f32,
            MARGIN as f32,
            (CHART_W - MARGIN) as f32,
            (CHART_H - MARGIN - X_LABEL_AREA) as f32,
        )
    }

    #[allow(dead_code)] // consumed from Task 4 (canvas); remove this allow then
    pub fn in_plot_rect(px: f32, py: f32) -> bool {
        let (x0, y0, x1, y1) = Self::plot_rect();
        (x0..=x1).contains(&px) && (y0..=y1).contains(&py)
    }

    #[allow(dead_code)] // consumed from Task 4 (canvas); remove this allow then
    pub fn pixel_to_data(&self, px: f32, py: f32) -> (f64, f64) {
        let (x0, y0, x1, y1) = Self::plot_rect();
        let fx = f64::from((px - x0) / (x1 - x0));
        let fy = f64::from((y1 - py) / (y1 - y0)); // pixel y grows downward
        (fx * self.x_max, fy * self.y_max)
    }

    #[allow(dead_code)] // consumed from Task 4 (canvas); remove this allow then
    pub fn data_to_pixel(&self, x: f64, y: f64) -> (f32, f32) {
        let (x0, y0, x1, y1) = Self::plot_rect();
        let px = x0 + ((x / self.x_max) as f32) * (x1 - x0);
        let py = y1 - ((y / self.y_max) as f32) * (y1 - y0);
        (px, py)
    }

    /// Which side of the cursor the readout box goes: past the horizontal
    /// midline it flips left; above the vertical midline it flips below.
    #[allow(dead_code)] // consumed from Task 4 (canvas); remove this allow then
    pub fn readout_flips(px: f32, py: f32) -> (bool, bool) {
        let (x0, y0, x1, y1) = Self::plot_rect();
        (px > (x0 + x1) / 2.0, py < (y0 + y1) / 2.0)
    }
}

/// Uniform-scale centered fit of the bitmap into the widget bounds.
#[allow(dead_code)] // consumed from Task 4 (canvas); remove this allow then
pub struct Letterbox {
    pub scale: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

#[allow(dead_code)] // consumed from Task 4 (canvas); remove this allow then
pub fn letterbox(bounds_w: f32, bounds_h: f32) -> Letterbox {
    let scale = (bounds_w / CHART_W as f32).min(bounds_h / CHART_H as f32);
    Letterbox {
        scale,
        offset_x: (bounds_w - CHART_W as f32 * scale) / 2.0,
        offset_y: (bounds_h - CHART_H as f32 * scale) / 2.0,
    }
}

impl Letterbox {
    #[allow(dead_code)] // consumed from Task 4 (canvas); remove this allow then
    pub fn widget_to_bitmap(&self, wx: f32, wy: f32) -> (f32, f32) {
        (
            (wx - self.offset_x) / self.scale,
            (wy - self.offset_y) / self.scale,
        )
    }

    #[allow(dead_code)] // consumed from Task 4 (canvas); remove this allow then
    pub fn bitmap_to_widget(&self, bx: f32, by: f32) -> (f32, f32) {
        (
            bx * self.scale + self.offset_x,
            by * self.scale + self.offset_y,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn plot_rect_derives_from_shared_constants() {
        // margin 24 + y-label 64 = 88 left; 760−24 = 736 right; top 24; 300−24−44 = 232 bottom.
        assert_eq!(ChartMapping::plot_rect(), (88.0, 24.0, 736.0, 232.0));
    }

    #[test]
    fn pixel_data_round_trip_is_identity() {
        let m = ChartMapping {
            x_max: 16.5,
            y_max: 33.0,
        };
        let (px, py) = m.data_to_pixel(12.3, 4.56);
        let (x, y) = m.pixel_to_data(px, py);
        assert_relative_eq!(x, 12.3, max_relative = 1e-6);
        assert_relative_eq!(y, 4.56, max_relative = 1e-6);
    }

    #[test]
    fn data_origin_maps_to_plot_rect_bottom_left() {
        let m = ChartMapping {
            x_max: 10.0,
            y_max: 10.0,
        };
        let (px, py) = m.data_to_pixel(0.0, 0.0);
        let (x0, _y0, _x1, y1) = ChartMapping::plot_rect();
        assert_relative_eq!(px, x0, max_relative = 1e-6);
        assert_relative_eq!(py, y1, max_relative = 1e-6); // y is inverted: data 0 sits at the BOTTOM
    }

    #[test]
    fn in_plot_rect_excludes_label_bands() {
        assert!(ChartMapping::in_plot_rect(400.0, 128.0));
        assert!(!ChartMapping::in_plot_rect(40.0, 128.0)); // inside y-label band
        assert!(!ChartMapping::in_plot_rect(400.0, 260.0)); // inside x-label band
    }

    #[test]
    fn readout_flips_at_plot_midlines() {
        let (fx, fy) = ChartMapping::readout_flips(700.0, 200.0); // right half, lower half
        assert!(fx, "near the right edge the box must flip left");
        assert!(!fy, "in the lower half the box sits above (no flip)");
        let (fx2, fy2) = ChartMapping::readout_flips(100.0, 30.0); // left half, top half
        assert!(!fx2);
        assert!(fy2, "near the top edge the box must flip below");
    }

    #[test]
    fn letterbox_round_trip_and_centering() {
        let lb = letterbox(1520.0, 300.0); // 2× width, exact height → scale 1.0? No: fit → scale = min(2.0, 1.0) = 1.0, centered horizontally
        assert_relative_eq!(lb.scale, 1.0, max_relative = 1e-6);
        assert_relative_eq!(lb.offset_x, 380.0, max_relative = 1e-6);
        let (bx, by) = lb.widget_to_bitmap(380.0 + 88.0, 24.0);
        assert_relative_eq!(bx, 88.0, max_relative = 1e-6);
        assert_relative_eq!(by, 24.0, max_relative = 1e-6);
        let (wx, wy) = lb.bitmap_to_widget(88.0, 24.0);
        assert_relative_eq!(wx, 468.0, max_relative = 1e-6);
        assert_relative_eq!(wy, 24.0, max_relative = 1e-6);
    }
}
