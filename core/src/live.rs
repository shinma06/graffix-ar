//! Live AR wall texture: the paint image the shell lays on the locked wall.
//!
//! Uses the same `raster` stamping as saved-work preview, timelapse and video
//! export, in the same per-stroke paint order as `Timeline` (dabs and drips
//! merged by recorded time, dabs first on ties), so the live wall and saved
//! works render identically. Rendering is incremental: each `update` stamps
//! only the marks added since the previous call and re-encodes only the
//! pixels they touch. Everything else — undo/redo/clear/load, paint growing
//! past the image extent, a new mark that would not land at the end of the
//! paint order — falls back to a full re-render.
//!
//! The image extent is the paint's bounds plus `PAD_M`, snapped outward to an
//! `EXTENT_STEP_M` wall grid, so it is a pure function of the paint and only
//! changes (forcing a full re-render) when paint crosses a grid line.

use crate::canvas::{Dab, Drip, Stroke, StrokeTiming};
use crate::math::Vec2;
use crate::palette;
use crate::raster::{linear_to_srgb, Paint, PaintLayer, View};
use crate::scene::Bounds;
use crate::timelapse::MarkKind;

/// Image density, pixels per wall meter, unless `MAX_SIDE` lowers it.
pub const PIXELS_PER_METER: f32 = 256.0;
/// Least transparent margin around the paint, meters.
pub const PAD_M: f32 = 0.10;
/// Wall grid the image extent snaps outward to, meters.
pub const EXTENT_STEP_M: f32 = 0.25;
/// Longest image side, pixels. Larger extents lower the density instead.
pub const MAX_SIDE: u32 = 2048;

/// Pixel rectangle x0..x1, y0..y1 (exclusive ends).
type PixelBox = (u32, u32, u32, u32);

/// How far the image reflects the strokes: strokes before `stroke` fully,
/// stroke `stroke` up to `dabs` dabs and `drips` drips.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Cursor {
    stroke: usize,
    dabs: usize,
    drips: usize,
}

#[derive(Default)]
pub struct LiveRaster {
    layer: Option<PaintLayer>,
    /// Premultiplied sRGB RGBA8 encoding of `layer`.
    rgba: Vec<u8>,
    /// `Canvas::rewrites` the image reflects; `None` before the first render.
    rewrites: Option<u64>,
    cursor: Cursor,
    /// Bounds of every stamped mark.
    bounds: Option<Bounds>,
    /// Scratch: (stroke index, mark) in paint order.
    pending: Vec<(usize, MarkKind)>,
    full_renders: u64,
}

impl LiveRaster {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bring the image up to date with `strokes` (committed strokes, then the
    /// in-flight one). `rewrites` is the canvas's `Canvas::rewrites`.
    pub fn update(&mut self, strokes: &[&Stroke], rewrites: u64) {
        if self.rewrites != Some(rewrites) || !self.update_incremental(strokes) {
            self.render_full(strokes);
            self.rewrites = Some(rewrites);
        }
    }

    /// The current image: its view (wall rectangle and density) and
    /// premultiplied sRGB RGBA8 pixels, row-major from the top-left (+v up).
    /// `None` when there is no paint.
    pub fn image(&self) -> Option<(View, &[u8])> {
        self.layer.as_ref().map(|l| (l.view, self.rgba.as_slice()))
    }

    /// Number of full re-renders so far.
    #[allow(dead_code)] // tests and the timing probe
    pub fn full_renders(&self) -> u64 {
        self.full_renders
    }

    /// Stamp only the marks added since the last update. `false` when that
    /// cannot reproduce a full render (the caller then re-renders).
    fn update_incremental(&mut self, strokes: &[&Stroke]) -> bool {
        let c = self.cursor;
        match strokes.get(c.stroke) {
            Some(s) => {
                if s.dabs.len() < c.dabs
                    || s.drips.len() < c.drips
                    || !is_order_prefix(s, c.dabs, c.drips)
                {
                    return false;
                }
            }
            None if strokes.len() == c.stroke && c.dabs == 0 && c.drips == 0 => {}
            None => return false,
        }

        let mut pending = std::mem::take(&mut self.pending);
        pending.clear();
        let mut bounds = self.bounds;
        for (i, s) in strokes.iter().enumerate().skip(c.stroke) {
            let from = if i == c.stroke {
                (c.dabs, c.drips)
            } else {
                (0, 0)
            };
            push_marks(i, s, from, &mut pending, &mut bounds);
        }
        let ok = self.stamp_pending(strokes, &pending, bounds);
        self.pending = pending;
        ok
    }

    fn stamp_pending(
        &mut self,
        strokes: &[&Stroke],
        pending: &[(usize, MarkKind)],
        bounds: Option<Bounds>,
    ) -> bool {
        if pending.is_empty() {
            return true;
        }
        let Some(layer) = self.layer.as_mut() else {
            return false;
        };
        if bounds.and_then(view_for) != Some(layer.view) {
            return false;
        }
        let mut dirty: Option<PixelBox> = None;
        stamp(layer, strokes, pending, Some(&mut dirty));
        if let Some(rect) = dirty {
            encode(layer, &mut self.rgba, rect);
        }
        self.bounds = bounds;
        self.cursor = end_cursor(strokes);
        true
    }

    fn render_full(&mut self, strokes: &[&Stroke]) {
        self.full_renders += 1;
        let mut pending = std::mem::take(&mut self.pending);
        pending.clear();
        let mut bounds = None;
        for (i, s) in strokes.iter().enumerate() {
            push_marks(i, s, (0, 0), &mut pending, &mut bounds);
        }
        self.cursor = end_cursor(strokes);
        self.bounds = bounds;
        match bounds.and_then(view_for) {
            None => {
                self.layer = None;
                self.rgba.clear();
            }
            Some(view) => {
                // Reuse the previous allocations.
                let mut layer = self.layer.take().unwrap_or_else(|| PaintLayer {
                    view,
                    px: Vec::new(),
                });
                layer.view = view;
                let n = view.width as usize * view.height as usize;
                layer.px.clear();
                layer.px.resize(n, [0.0; 4]);
                stamp(&mut layer, strokes, &pending, None);
                self.rgba.resize(n * 4, 0);
                encode(&layer, &mut self.rgba, (0, view.width, 0, view.height));
                self.layer = Some(layer);
            }
        }
        self.pending = pending;
    }
}

fn end_cursor(strokes: &[&Stroke]) -> Cursor {
    match strokes.last() {
        Some(s) => Cursor {
            stroke: strokes.len() - 1,
            dabs: s.dabs.len(),
            drips: s.drips.len(),
        },
        None => Cursor::default(),
    }
}

/// Recorded timing, when it covers every mark (as `Timeline` requires).
fn timing(s: &Stroke) -> Option<&StrokeTiming> {
    s.timing
        .as_ref()
        .filter(|t| t.dab_ms.len() == s.dabs.len() && t.drip_ms.len() == s.drips.len())
}

/// Whether the next mark in paint order after `d` dabs and `r` drips is a
/// dab; `None` when the stroke is exhausted. Same merge as `Timeline::build`.
fn next_is_dab(s: &Stroke, t: Option<&StrokeTiming>, d: usize, r: usize) -> Option<bool> {
    let (nd, nr) = (s.dabs.len(), s.drips.len());
    if d >= nd && r >= nr {
        return None;
    }
    Some(match t {
        Some(t) => r >= nr || (d < nd && t.dab_ms[d] <= t.drip_ms[r]),
        None => d < nd,
    })
}

/// Whether the first `d` dabs and `r` drips form a prefix of the stroke's
/// paint order, i.e. stamping the rest after them matches a full render.
fn is_order_prefix(s: &Stroke, d: usize, r: usize) -> bool {
    let t = timing(s);
    let (mut i, mut j) = (0, 0);
    while i < d || j < r {
        match next_is_dab(s, t, i, j) {
            Some(true) => i += 1,
            Some(false) => j += 1,
            None => return false,
        }
        if i > d || j > r {
            return false;
        }
    }
    true
}

/// Append stroke `i`'s marks after `from` (dabs, drips) in paint order.
fn push_marks(
    i: usize,
    s: &Stroke,
    from: (usize, usize),
    out: &mut Vec<(usize, MarkKind)>,
    bounds: &mut Option<Bounds>,
) {
    let t = timing(s);
    let (mut d, mut r) = from;
    while let Some(is_dab) = next_is_dab(s, t, d, r) {
        if is_dab {
            add_bounds(bounds, dab_bounds(&s.dabs[d]));
            out.push((i, MarkKind::Dab(d as u32)));
            d += 1;
        } else {
            add_bounds(bounds, drip_bounds(&s.drips[r]));
            out.push((i, MarkKind::Drip(r as u32)));
            r += 1;
        }
    }
}

/// Same extents as `Bounds::of`.
fn dab_bounds(d: &Dab) -> Bounds {
    let r = d.radius_major.max(d.radius_minor);
    Bounds {
        min: Vec2::new(d.uv.x - r, d.uv.y - r),
        max: Vec2::new(d.uv.x + r, d.uv.y + r),
    }
}

fn drip_bounds(d: &Drip) -> Bounds {
    let w = d.width * 0.5;
    Bounds {
        min: Vec2::new(d.uv.x - w, d.uv.y - d.len),
        max: Vec2::new(d.uv.x + w, d.uv.y + w),
    }
}

fn add_bounds(acc: &mut Option<Bounds>, b: Bounds) {
    *acc = Some(match *acc {
        None => b,
        Some(o) => Bounds {
            min: Vec2::new(o.min.x.min(b.min.x), o.min.y.min(b.min.y)),
            max: Vec2::new(o.max.x.max(b.max.x), o.max.y.max(b.max.y)),
        },
    });
}

/// The image view for paint `bounds`: padded, grid-snapped, square pixels at
/// `PIXELS_PER_METER` unless that would exceed `MAX_SIDE`.
fn view_for(b: Bounds) -> Option<View> {
    let snap_down = |x: f32| (x / EXTENT_STEP_M).floor() * EXTENT_STEP_M;
    let snap_up = |x: f32| (x / EXTENT_STEP_M).ceil() * EXTENT_STEP_M;
    let (min_u, max_u) = (snap_down(b.min.x - PAD_M), snap_up(b.max.x + PAD_M));
    let (min_v, max_v) = (snap_down(b.min.y - PAD_M), snap_up(b.max.y + PAD_M));
    let (w_m, h_m) = (max_u - min_u, max_v - min_v);
    if !(w_m.is_finite() && h_m.is_finite() && w_m > 0.0 && h_m > 0.0) {
        return None;
    }
    let m_per_px = (1.0 / PIXELS_PER_METER).max(w_m.max(h_m) / MAX_SIDE as f32);
    let side = |m: f32| ((m / m_per_px).round() as u32).clamp(1, MAX_SIDE);
    Some(View {
        width: side(w_m),
        height: side(h_m),
        left_u: min_u,
        top_v: max_v,
        m_per_px,
    })
}

/// Stamp `marks` into `layer`, growing `dirty` by the pixels touched.
fn stamp(
    layer: &mut PaintLayer,
    strokes: &[&Stroke],
    marks: &[(usize, MarkKind)],
    mut dirty: Option<&mut Option<PixelBox>>,
) {
    let mut current: Option<(usize, Paint)> = None;
    for &(i, kind) in marks {
        let s = strokes[i];
        let paint = match current {
            Some((ci, p)) if ci == i => p,
            _ => {
                let p = Paint::from(palette::color(s.palette_index));
                current = Some((i, p));
                p
            }
        };
        let touched = match kind {
            MarkKind::Dab(d) => {
                let dab = &s.dabs[d as usize];
                layer.stamp_dab(dab, paint);
                layer.view.dab_box(dab)
            }
            MarkKind::Drip(r) => {
                let drip = &s.drips[r as usize];
                layer.stamp_drip(drip, paint);
                layer.view.drip_box(drip)
            }
        };
        if let (Some(acc), Some(b)) = (dirty.as_deref_mut(), touched) {
            *acc = Some(match *acc {
                None => b,
                Some(a) => (a.0.min(b.0), a.1.max(b.1), a.2.min(b.2), a.3.max(b.3)),
            });
        }
    }
}

/// Re-encode `rect` of `layer` into `rgba` (premultiplied sRGB RGBA8).
fn encode(layer: &PaintLayer, rgba: &mut [u8], (x0, x1, y0, y1): PixelBox) {
    let w = layer.view.width as usize;
    for y in y0 as usize..y1 as usize {
        let row = y * w;
        for x in x0 as usize..x1 as usize {
            let i = row + x;
            rgba[i * 4..i * 4 + 4].copy_from_slice(&encode_px(layer.px[i]));
        }
    }
}

/// Premultiplied linear RGBA -> premultiplied sRGB RGBA8.
fn encode_px(p: [f32; 4]) -> [u8; 4] {
    let a = p[3].clamp(0.0, 1.0);
    if a <= 0.0 {
        return [0; 4];
    }
    let ch = |c: f32| (linear_to_srgb(c / a) * a * 255.0 + 0.5) as u8;
    [ch(p[0]), ch(p[1]), ch(p[2]), (a * 255.0 + 0.5) as u8]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timelapse::Timeline;

    /// A recorded stroke laid out like the engine's: dabs along +u at 16 ms,
    /// a drip after every `drip_every`-th dab at that dab's time.
    fn stroke(palette_index: u8, origin: Vec2, n: usize, drip_every: usize) -> Stroke {
        let mut s = Stroke {
            palette_index,
            dabs: Vec::new(),
            drips: Vec::new(),
            timing: Some(StrokeTiming::default()),
        };
        for k in 0..n {
            grow(&mut s, origin, k, drip_every);
        }
        s
    }

    /// Append dab `k` (and its drip) to `s`.
    fn grow(s: &mut Stroke, origin: Vec2, k: usize, drip_every: usize) {
        let t = s.timing.as_mut().unwrap();
        let ms = k as u32 * 16;
        s.dabs.push(Dab {
            uv: Vec2::new(
                origin.x + k as f32 * 0.012,
                origin.y + (k as f32 * 0.7).sin() * 0.05,
            ),
            radius_major: 0.03 + 0.01 * (k % 3) as f32,
            radius_minor: 0.025,
            rotation: k as f32 * 0.3,
            gradient: Vec2::new(0.6, 0.8),
            opacity: 0.7,
            softness: 0.4,
        });
        t.dab_ms.push(ms);
        if drip_every > 0 && k % drip_every == drip_every - 1 {
            s.drips.push(Drip {
                uv: s.dabs[k].uv,
                len: 0.08 + 0.01 * (k % 5) as f32,
                width: 0.006,
                opacity: 0.8,
            });
            t.drip_ms.push(ms);
        }
    }

    fn render(strokes: &[Stroke], rewrites: u64) -> LiveRaster {
        let mut r = LiveRaster::new();
        r.update(&strokes.iter().collect::<Vec<_>>(), rewrites);
        r
    }

    fn assert_same(a: &LiveRaster, b: &LiveRaster) {
        let (va, pa) = a.image().expect("a has an image");
        let (vb, pb) = b.image().expect("b has an image");
        assert_eq!(va, vb);
        assert_eq!(a.layer.as_ref().unwrap().px, b.layer.as_ref().unwrap().px);
        assert!(pa == pb, "rgba differs");
    }

    fn sample() -> Vec<Stroke> {
        vec![
            stroke(3, Vec2::new(0.0, 0.0), 30, 7),
            stroke(8, Vec2::new(0.05, 0.03), 25, 4),
        ]
    }

    #[test]
    fn empty_canvas_has_no_image() {
        assert!(render(&[], 0).image().is_none());
        let empty = Stroke {
            palette_index: 0,
            dabs: vec![],
            drips: vec![],
            timing: None,
        };
        let mut r = render(std::slice::from_ref(&empty), 0);
        assert!(r.image().is_none());
        // Paint appearing later is picked up; disappearing clears the image.
        let s = stroke(1, Vec2::new(0.0, 0.0), 3, 0);
        r.update(&[&s], 0);
        assert!(r.image().is_some());
        r.update(&[], 1);
        assert!(r.image().is_none());
    }

    #[test]
    fn incremental_matches_full_render() {
        let full = render(&sample(), 0);
        // Grow the same strokes mark by mark, updating at varying cadences.
        for cadence in [1, 2, 5, 13] {
            let mut live = LiveRaster::new();
            let mut strokes: Vec<Stroke> = Vec::new();
            let plan = [
                (3u8, Vec2::new(0.0, 0.0), 30usize, 7usize),
                (8, Vec2::new(0.05, 0.03), 25, 4),
            ];
            let mut step = 0;
            for &(color, origin, n, every) in &plan {
                strokes.push(stroke(color, origin, 0, every));
                for k in 0..n {
                    grow(strokes.last_mut().unwrap(), origin, k, every);
                    step += 1;
                    if step % cadence == 0 {
                        live.update(&strokes.iter().collect::<Vec<_>>(), 0);
                    }
                }
            }
            live.update(&strokes.iter().collect::<Vec<_>>(), 0);
            assert_same(&live, &full);
            // Growth within one grid cell is drawn incrementally.
            assert!(
                live.full_renders() < step as u64 / cadence as u64,
                "cadence {cadence}: {} full renders",
                live.full_renders()
            );
        }
    }

    #[test]
    fn unchanged_input_is_not_re_rendered() {
        let strokes = sample();
        let refs: Vec<&Stroke> = strokes.iter().collect();
        let mut r = LiveRaster::new();
        r.update(&refs, 0);
        r.update(&refs, 0);
        r.update(&refs, 0);
        assert_eq!(r.full_renders(), 1);
    }

    #[test]
    fn rewrites_force_a_full_render() {
        let mut strokes = sample();
        let mut live = render(&strokes, 0);
        // Undo: the last stroke disappears.
        let undone = strokes.pop().unwrap();
        live.update(&strokes.iter().collect::<Vec<_>>(), 1);
        assert_same(&live, &render(&strokes, 0));
        // Redo: it comes back.
        strokes.push(undone);
        live.update(&strokes.iter().collect::<Vec<_>>(), 2);
        assert_same(&live, &render(&strokes, 0));
        // Load: same stroke count and sizes, different paint.
        let loaded = vec![
            stroke(5, Vec2::new(0.0, 0.0), 30, 7),
            stroke(8, Vec2::new(0.05, 0.02), 25, 4),
        ];
        let before = live.full_renders();
        live.update(&loaded.iter().collect::<Vec<_>>(), 3);
        assert_eq!(live.full_renders(), before + 1);
        assert_same(&live, &render(&loaded, 0));
        // Clear.
        live.update(&[], 4);
        assert!(live.image().is_none());
    }

    #[test]
    fn growing_past_the_extent_re_renders_with_a_larger_image() {
        let mut s = stroke(2, Vec2::new(0.0, 0.0), 1, 0);
        let mut live = render(std::slice::from_ref(&s), 0);
        let (first, _) = live.image().unwrap();
        for k in 1..120 {
            grow(&mut s, Vec2::new(0.0, 0.0), k, 9);
            live.update(&[&s], 0);
        }
        let (grown, _) = live.image().unwrap();
        assert!(grown.width > first.width);
        assert!(live.full_renders() > 1);
        assert_same(&live, &render(std::slice::from_ref(&s), 0));
        // The image covers the paint with at least the pad.
        let b = live.bounds.unwrap();
        assert!(grown.left_u <= b.min.x - PAD_M);
        assert!(grown.left_u + grown.width_m() >= b.max.x + PAD_M - 1e-4);
        assert!(grown.top_v >= b.max.y + PAD_M);
        assert!(grown.top_v - grown.height_m() <= b.min.y - PAD_M + 1e-4);
    }

    #[test]
    fn extent_is_snapped_and_dense_by_default() {
        let s = stroke(2, Vec2::new(0.31, -0.2), 1, 0);
        let (v, rgba) = render(std::slice::from_ref(&s), 0)
            .image()
            .map(|(v, p)| (v, p.to_vec()))
            .unwrap();
        assert_eq!(v.m_per_px, 1.0 / PIXELS_PER_METER);
        for x in [v.left_u, v.top_v, v.width_m(), v.height_m()] {
            let cells = x / EXTENT_STEP_M;
            assert!((cells - cells.round()).abs() < 1e-4, "{x} is off-grid");
        }
        assert_eq!(rgba.len(), (v.width * v.height * 4) as usize);
        // Transparent margin, paint in the middle.
        assert_eq!(&rgba[..4], &[0, 0, 0, 0]);
        assert!(rgba.chunks(4).any(|p| p[3] > 0));
    }

    #[test]
    fn huge_extent_caps_the_side_and_keeps_square_pixels() {
        let mut s = stroke(2, Vec2::new(0.0, 0.0), 1, 0);
        s.dabs
            .push(Dab::circle(Vec2::new(30.0, 4.0), 0.05, 1.0, 0.5));
        s.timing.as_mut().unwrap().dab_ms.push(16);
        let live = render(std::slice::from_ref(&s), 0);
        let (v, rgba) = live.image().unwrap();
        assert_eq!(v.width, MAX_SIDE);
        assert!(v.height < MAX_SIDE && v.height > 0);
        assert!(v.m_per_px > 1.0 / PIXELS_PER_METER);
        assert!(v.width_m() >= 30.0);
        assert_eq!(rgba.len(), (v.width * v.height * 4) as usize);
        // Both far-apart dabs are still visible.
        let alpha = |u: f32, vv: f32| {
            let x = ((u - v.left_u) / v.m_per_px) as u32;
            let y = ((v.top_v - vv) / v.m_per_px) as u32;
            rgba[((y * v.width + x) * 4 + 3) as usize]
        };
        assert!(alpha(0.0, 0.0) > 0 && alpha(30.0, 4.0) > 0);
    }

    #[test]
    fn non_finite_paint_has_no_image() {
        let s = Stroke {
            palette_index: 0,
            dabs: vec![Dab::circle(Vec2::new(f32::INFINITY, 0.0), 0.05, 1.0, 0.5)],
            drips: vec![],
            timing: None,
        };
        assert!(render(&[s], 0).image().is_none());
    }

    #[test]
    fn rendering_is_deterministic() {
        let a = render(&sample(), 0);
        let b = render(&sample(), 7);
        assert_same(&a, &b);
    }

    #[test]
    fn out_of_order_new_mark_falls_back_to_a_full_render() {
        // A drip was stamped at t = 32 ms; the next dab is recorded at the
        // same time, which paint order puts before that drip.
        let mut s = stroke(4, Vec2::new(0.0, 0.0), 3, 3);
        assert_eq!(s.timing.as_ref().unwrap().drip_ms, vec![32]);
        let mut live = render(std::slice::from_ref(&s), 0);
        s.dabs
            .push(Dab::circle(Vec2::new(0.02, 0.0), 0.04, 0.9, 0.3));
        s.timing.as_mut().unwrap().dab_ms.push(32);
        live.update(&[&s], 0);
        assert_eq!(live.full_renders(), 2);
        assert_same(&live, &render(std::slice::from_ref(&s), 0));
    }

    #[test]
    fn untimed_strokes_render_dabs_then_drips() {
        let mut s = stroke(6, Vec2::new(0.0, 0.0), 12, 4);
        s.timing = None;
        let mut live = render(std::slice::from_ref(&s), 0);
        // A dab after the drips cannot be appended in order.
        s.dabs
            .push(Dab::circle(Vec2::new(0.03, 0.0), 0.04, 0.9, 0.3));
        live.update(&[&s], 0);
        assert_eq!(live.full_renders(), 2);
        assert_same(&live, &render(std::slice::from_ref(&s), 0));
    }

    #[test]
    fn matches_the_saved_work_rasterizer() {
        let mut strokes = sample();
        let mut untimed = stroke(10, Vec2::new(-0.1, 0.1), 9, 3);
        untimed.timing = None;
        strokes.push(untimed);
        let live = render(&strokes, 0);
        let (view, _) = live.image().unwrap();
        // What `SavedWork` stamps for previews and timelapse frames.
        let mut saved = PaintLayer::new(view);
        for m in Timeline::build(&strokes).marks {
            let s = &strokes[m.stroke as usize];
            let paint = Paint::from(palette::color(s.palette_index));
            match m.kind {
                MarkKind::Dab(i) => saved.stamp_dab(&s.dabs[i as usize], paint),
                MarkKind::Drip(i) => saved.stamp_drip(&s.drips[i as usize], paint),
            }
        }
        assert_eq!(live.layer.as_ref().unwrap().px, saved.px);
    }

    #[test]
    fn encoding_is_premultiplied_srgb() {
        assert_eq!(encode_px([0.0; 4]), [0, 0, 0, 0]);
        assert_eq!(encode_px([1.0, 0.0, 0.0, 1.0]), [255, 0, 0, 255]);
        // Half-covered linear 0.214 (sRGB ~0.5): stored halved.
        let p = encode_px([0.107, 0.107, 0.107, 0.5]);
        assert_eq!(p[3], 128);
        assert!((p[0] as i32 - 64).abs() <= 1, "{p:?}");
        // Premultiplied: color never exceeds alpha.
        for a in [0.01f32, 0.2, 0.7] {
            let p = encode_px([a, a * 0.5, 0.0, a]);
            assert!(p[0] <= p[3] && p[1] <= p[3]);
        }
    }

    /// Per-frame cost probe:
    /// `cargo test --release --lib live_timing -- --ignored --nocapture`.
    /// Sprays a 1 m x 0.8 m zigzag at one dab per frame.
    #[test]
    #[ignore]
    fn live_timing() {
        use std::time::Instant;
        let mut strokes = vec![stroke(1, Vec2::new(0.0, 0.0), 0, 6)];
        let mut live = LiveRaster::new();
        let frames = 2000;
        let mut worst = std::time::Duration::ZERO;
        let t0 = Instant::now();
        for k in 0..frames {
            let s = &mut strokes[0];
            grow(s, Vec2::new(0.0, 0.0), k, 6);
            let row = (k / 100) as f32;
            let col = (k % 100) as f32;
            let x = if (row as usize).is_multiple_of(2) {
                col
            } else {
                99.0 - col
            };
            let uv = Vec2::new(x * 0.01, -row * 0.04);
            s.dabs[k].uv = uv;
            if s.drips.len() * 6 == k + 1 {
                s.drips.last_mut().unwrap().uv = uv;
            }
            let t = Instant::now();
            live.update(&strokes.iter().collect::<Vec<_>>(), 0);
            worst = worst.max(t.elapsed());
        }
        let per_frame = t0.elapsed() / frames as u32;
        let (v, _) = live.image().unwrap();
        let t1 = Instant::now();
        let full = render(&strokes, 0);
        let full_time = t1.elapsed();
        assert_same(&live, &full);
        println!(
            "{}x{} px, {} full renders: {:?}/frame mean, {:?} worst, {:?} full render",
            v.width,
            v.height,
            live.full_renders(),
            per_frame,
            worst,
            full_time
        );
    }
}
