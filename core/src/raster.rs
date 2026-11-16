//! Software rasterizer shared by the live AR wall texture (`live`), saved-work
//! scene preview and timelapse (`work`, `scene`, `export`).
//!
//! Each dab is an ellipse filled with a radial gradient whose dense core is
//! offset toward the near side, and each drip is a capsule with a top-down
//! fade. Paint accumulates source-over into a premultiplied, linear-light
//! layer that `live` encodes as a transparent texture and `scene` composites
//! onto a simulated wall.

use crate::canvas::{Dab, Drip};
use crate::ffi::ColorF;
use crate::math::Vec2;

/// Maps output pixels to wall-local meters (u right, v up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub width: u32,
    pub height: u32,
    /// Wall coordinates of the image's top-left corner.
    pub left_u: f32,
    pub top_v: f32,
    /// Wall meters per pixel (square pixels).
    pub m_per_px: f32,
}

impl View {
    /// A view of `width` x `height` pixels centered on `center` covering
    /// `m_per_px` meters per pixel.
    pub fn centered(center: Vec2, m_per_px: f32, width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            left_u: center.x - m_per_px * width as f32 * 0.5,
            top_v: center.y + m_per_px * height as f32 * 0.5,
            m_per_px,
        }
    }

    /// Wall position of a pixel center.
    pub fn wall_at(&self, x: u32, y: u32) -> Vec2 {
        Vec2::new(
            self.left_u + (x as f32 + 0.5) * self.m_per_px,
            self.top_v - (y as f32 + 0.5) * self.m_per_px,
        )
    }

    pub fn width_m(&self) -> f32 {
        self.width as f32 * self.m_per_px
    }

    pub fn height_m(&self) -> f32 {
        self.height as f32 * self.m_per_px
    }

    /// Pixel rectangle (x0..x1, y0..y1, exclusive ends) covering a wall
    /// rectangle, clipped to the image. `None` when fully outside.
    pub(crate) fn pixel_box(
        &self,
        min_u: f32,
        max_u: f32,
        min_v: f32,
        max_v: f32,
    ) -> Option<(u32, u32, u32, u32)> {
        let fx0 = ((min_u - self.left_u) / self.m_per_px).floor() - 1.0;
        let fx1 = ((max_u - self.left_u) / self.m_per_px).ceil() + 1.0;
        let fy0 = ((self.top_v - max_v) / self.m_per_px).floor() - 1.0;
        let fy1 = ((self.top_v - min_v) / self.m_per_px).ceil() + 1.0;
        let (w, h) = (self.width as f32, self.height as f32);
        if !(fx1 > 0.0 && fy1 > 0.0 && fx0 < w && fy0 < h) {
            return None;
        }
        Some((
            fx0.max(0.0) as u32,
            fx1.min(w) as u32,
            fy0.max(0.0) as u32,
            fy1.min(h) as u32,
        ))
    }

    /// Pixels `PaintLayer::stamp_dab` may touch.
    pub(crate) fn dab_box(&self, dab: &Dab) -> Option<(u32, u32, u32, u32)> {
        let min_r = 1.25 * self.m_per_px;
        let r = dab.radius_major.max(min_r).max(dab.radius_minor.max(min_r));
        self.pixel_box(dab.uv.x - r, dab.uv.x + r, dab.uv.y - r, dab.uv.y + r)
    }

    /// A drip's capsule as (center, half size), widened to at least a pixel.
    fn drip_capsule(&self, drip: &Drip) -> (Vec2, Vec2) {
        let w = drip.width.max(self.m_per_px);
        let half = Vec2::new(w * 0.5, drip.len * 0.5);
        (Vec2::new(drip.uv.x, drip.uv.y - half.y), half)
    }

    /// Pixels `PaintLayer::stamp_drip` may touch.
    pub(crate) fn drip_box(&self, drip: &Drip) -> Option<(u32, u32, u32, u32)> {
        let (center, half) = self.drip_capsule(drip);
        self.pixel_box(
            center.x - half.x,
            center.x + half.x,
            center.y - half.y,
            center.y + half.y,
        )
    }
}

/// sRGB-encoded 0..1 -> linear light.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light -> sRGB-encoded 0..1.
pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Paint color in linear light plus its straight alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paint {
    pub rgb: [f32; 3],
    pub alpha: f32,
}

impl From<ColorF> for Paint {
    fn from(c: ColorF) -> Self {
        Self {
            rgb: [
                srgb_to_linear(c.r as f32 / 255.0),
                srgb_to_linear(c.g as f32 / 255.0),
                srgb_to_linear(c.b as f32 / 255.0),
            ],
            alpha: c.a as f32 / 255.0,
        }
    }
}

/// Accumulated paint: premultiplied linear RGB + coverage alpha per pixel.
#[derive(Clone, Debug)]
pub struct PaintLayer {
    pub view: View,
    pub px: Vec<[f32; 4]>,
}

impl PaintLayer {
    pub fn new(view: View) -> Self {
        Self {
            view,
            px: vec![[0.0; 4]; view.width as usize * view.height as usize],
        }
    }

    pub fn get(&self, x: u32, y: u32) -> [f32; 4] {
        self.px[(y * self.view.width + x) as usize]
    }

    fn blend(&mut self, x: u32, y: u32, paint: Paint, alpha: f32) {
        let a = (alpha * paint.alpha).clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        let p = &mut self.px[(y * self.view.width + x) as usize];
        for (pc, &rc) in p.iter_mut().zip(&paint.rgb) {
            *pc = rc * a + *pc * (1.0 - a);
        }
        p[3] = a + p[3] * (1.0 - a);
    }

    /// One v2 dab. Dabs smaller than ~a pixel are widened to stay visible,
    /// with alpha scaled down so the deposited paint stays the same.
    pub fn stamp_dab(&mut self, dab: &Dab, paint: Paint) {
        let v = self.view;
        let min_r = 1.25 * v.m_per_px;
        let a = dab.radius_major.max(min_r);
        let b = dab.radius_minor.max(min_r);
        if dab.radius_major <= 0.0 || dab.radius_minor <= 0.0 || dab.opacity <= 0.0 {
            return;
        }
        let area_scale = (dab.radius_major * dab.radius_minor) / (a * b);
        let peak = dab.opacity * area_scale;
        let edge = (1.0 - 0.65 * dab.softness).min(0.98);
        let (sin_t, cos_t) = dab.rotation.sin_cos();
        // Dense-side focus of the radial gradient, in unit-ellipse space.
        let gx = cos_t * dab.gradient.x + sin_t * dab.gradient.y;
        let gy = -sin_t * dab.gradient.x + cos_t * dab.gradient.y;
        let s = Vec2::new(gx * 0.4, gy * 0.4);
        let Some((x0, x1, y0, y1)) = v.dab_box(dab) else {
            return;
        };
        for y in y0..y1 {
            for x in x0..x1 {
                let p = v.wall_at(x, y) - dab.uv;
                let q = Vec2::new(
                    (cos_t * p.x + sin_t * p.y) / a,
                    (-sin_t * p.x + cos_t * p.y) / b,
                );
                let t = radial_t(q, s);
                let alpha = gradient_alpha(t, edge, peak);
                if alpha > 0.0 {
                    self.blend(x, y, paint, alpha);
                }
            }
        }
    }

    /// One drip: a capsule from its head down `len`, fading toward the tail.
    pub fn stamp_drip(&mut self, drip: &Drip, paint: Paint) {
        let v = self.view;
        if drip.len <= 0.0 || drip.width <= 0.0 || drip.opacity <= 0.0 {
            return;
        }
        let (center, half) = v.drip_capsule(drip);
        let alpha_scale = drip.width / (half.x * 2.0);
        let radius = half.x.min(half.y);
        let Some((x0, x1, y0, y1)) = v.drip_box(drip) else {
            return;
        };
        for y in y0..y1 {
            for x in x0..x1 {
                let p = v.wall_at(x, y);
                let d = p - center;
                let qx = (d.x.abs() - (half.x - radius)).max(0.0);
                let qy = (d.y.abs() - (half.y - radius)).max(0.0);
                let sd = (qx * qx + qy * qy).sqrt() - radius;
                let coverage = (0.5 - sd / v.m_per_px).clamp(0.0, 1.0);
                if coverage <= 0.0 {
                    continue;
                }
                // 0 at the head, 1 at the tail.
                let t = ((drip.uv.y - p.y) / drip.len).clamp(0.0, 1.0);
                let fade = if t < 0.55 {
                    1.0 - 0.4 * (t / 0.55)
                } else {
                    0.6 * (1.0 - (t - 0.55) / 0.45)
                };
                self.blend(x, y, paint, drip.opacity * fade * coverage * alpha_scale);
            }
        }
    }
}

/// Gradient parameter of point `q` for a radial gradient whose start circle
/// is the point `s` (radius 0) and whose end circle is the unit circle at the
/// origin — the same geometry CoreGraphics' two-point radial gradient uses.
/// Solves |q - s(1 - t)| = t for the non-negative root.
fn radial_t(q: Vec2, s: Vec2) -> f32 {
    let d = q - s;
    let a = s.x * s.x + s.y * s.y - 1.0; // < 0 because |s| <= 0.4
    let b = 2.0 * (s.x * d.x + s.y * d.y);
    let c = d.x * d.x + d.y * d.y;
    let disc = (b * b - 4.0 * a * c).max(0.0);
    (-b - disc.sqrt()) / (2.0 * a)
}

/// Dab alpha at gradient parameter `t`: stops [0: peak, edge: 0.55 peak,
/// 1: 0], transparent beyond the rim.
fn gradient_alpha(t: f32, edge: f32, peak: f32) -> f32 {
    if t >= 1.0 {
        0.0
    } else if t <= edge {
        peak * (1.0 - 0.45 * (t / edge))
    } else {
        peak * 0.55 * (1.0 - (t - edge) / (1.0 - edge))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: ColorF = ColorF {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };

    fn view_1cm(w: u32, h: u32) -> View {
        View::centered(Vec2::new(0.0, 0.0), 0.01, w, h)
    }

    fn alpha_at(layer: &PaintLayer, u: f32, v: f32) -> f32 {
        let view = layer.view;
        let x = ((u - view.left_u) / view.m_per_px) as u32;
        let y = ((view.top_v - v) / view.m_per_px) as u32;
        layer.get(x, y)[3]
    }

    #[test]
    fn view_maps_pixel_centers_to_wall() {
        let v = view_1cm(100, 50);
        assert!((v.width_m() - 1.0).abs() < 1e-6);
        assert!((v.height_m() - 0.5).abs() < 1e-6);
        let tl = v.wall_at(0, 0);
        assert!((tl.x + 0.495).abs() < 1e-5 && (tl.y - 0.245).abs() < 1e-5);
        let c = v.wall_at(50, 25);
        assert!(c.x.abs() < 0.01 && c.y.abs() < 0.01);
    }

    #[test]
    fn srgb_round_trip() {
        for i in 0..=255 {
            let c = i as f32 / 255.0;
            assert!((linear_to_srgb(srgb_to_linear(c)) - c).abs() < 1e-4);
        }
    }

    #[test]
    fn radial_t_matches_circle_when_centered() {
        let t = radial_t(Vec2::new(0.6, 0.0), Vec2::new(0.0, 0.0));
        assert!((t - 0.6).abs() < 1e-6);
        // With the focus offset to +x, a point on the +x side reaches the
        // rim later in t than the mirrored -x point... i.e. it is denser.
        let s = Vec2::new(0.4, 0.0);
        assert!(radial_t(Vec2::new(0.5, 0.0), s) < radial_t(Vec2::new(-0.5, 0.0), s));
        // The rim is t = 1 regardless of the focus.
        assert!((radial_t(Vec2::new(0.0, 1.0), s) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn dab_is_dense_in_core_and_empty_outside() {
        let mut layer = PaintLayer::new(view_1cm(64, 64));
        let dab = Dab::circle(Vec2::new(0.0, 0.0), 0.1, 0.8, 0.5);
        layer.stamp_dab(&dab, RED.into());
        let center = alpha_at(&layer, 0.0, 0.0);
        assert!((center - 0.8).abs() < 0.05, "center alpha {center}");
        assert!(alpha_at(&layer, 0.05, 0.0) < center);
        assert_eq!(alpha_at(&layer, 0.15, 0.0), 0.0);
        let p = layer.get(32, 32);
        assert!(p[0] > 0.7 && p[1] == 0.0 && p[2] == 0.0);
    }

    #[test]
    fn gradient_makes_near_side_denser() {
        let mut layer = PaintLayer::new(view_1cm(64, 64));
        let dab = Dab {
            uv: Vec2::new(0.0, 0.0),
            radius_major: 0.12,
            radius_minor: 0.08,
            rotation: 0.0,
            gradient: Vec2::new(1.0, 0.0),
            opacity: 0.9,
            softness: 0.5,
        };
        layer.stamp_dab(&dab, RED.into());
        assert!(alpha_at(&layer, 0.07, 0.0) > alpha_at(&layer, -0.07, 0.0));
    }

    #[test]
    fn rotated_ellipse_extends_along_major_axis() {
        let mut layer = PaintLayer::new(view_1cm(64, 64));
        let dab = Dab {
            uv: Vec2::new(0.0, 0.0),
            radius_major: 0.2,
            radius_minor: 0.05,
            rotation: std::f32::consts::FRAC_PI_2, // major along +v
            gradient: Vec2::new(0.0, 0.0),
            opacity: 1.0,
            softness: 0.0,
        };
        layer.stamp_dab(&dab, RED.into());
        assert!(alpha_at(&layer, 0.0, 0.15) > 0.0);
        assert_eq!(alpha_at(&layer, 0.15, 0.0), 0.0);
    }

    #[test]
    fn sub_pixel_dab_keeps_its_paint() {
        // 2 mm dab on a 1 cm/px view: widened to stay visible, but the total
        // deposited alpha stays close to the true dab's.
        let mut layer = PaintLayer::new(view_1cm(16, 16));
        let dab = Dab::circle(Vec2::new(0.0, 0.0), 0.002, 1.0, 0.5);
        layer.stamp_dab(&dab, RED.into());
        let total: f32 = layer.px.iter().map(|p| p[3]).sum();
        let mut fine = PaintLayer::new(View::centered(Vec2::new(0.0, 0.0), 0.0005, 64, 64));
        fine.stamp_dab(&dab, RED.into());
        let fine_total: f32 =
            fine.px.iter().map(|p| p[3]).sum::<f32>() * (0.0005f32 / 0.01).powi(2);
        assert!(total > 0.0);
        assert!(
            (total / fine_total - 1.0).abs() < 0.35,
            "{total} vs {fine_total}"
        );
    }

    #[test]
    fn drip_runs_down_and_fades() {
        let mut layer = PaintLayer::new(view_1cm(40, 80));
        let drip = Drip {
            uv: Vec2::new(0.0, 0.3),
            len: 0.5,
            width: 0.03,
            opacity: 1.0,
        };
        layer.stamp_drip(&drip, RED.into());
        let head = alpha_at(&layer, 0.0, 0.25);
        let mid = alpha_at(&layer, 0.0, 0.0);
        let tail = alpha_at(&layer, 0.0, -0.18);
        assert!(
            head > mid && mid > tail && tail > 0.0,
            "{head} {mid} {tail}"
        );
        assert_eq!(alpha_at(&layer, 0.0, 0.34), 0.0); // above the head
        assert_eq!(alpha_at(&layer, 0.1, 0.0), 0.0); // beside the streak
    }

    #[test]
    fn out_of_view_marks_are_ignored() {
        let mut layer = PaintLayer::new(view_1cm(8, 8));
        layer.stamp_dab(&Dab::circle(Vec2::new(5.0, 5.0), 0.1, 1.0, 0.5), RED.into());
        assert!(layer.px.iter().all(|p| p[3] == 0.0));
    }
}
