//! The spray-can model, v2: two modes (casual presets / real physics), cap
//! selection, incidence-angle ellipses, speed thinning, can-shake speckle,
//! and drip triggering. Dabs are spaced along a stroke as before.
//!
//! Physical picture: a spray can emits a cone of paint. The disc the cone
//! cuts on the wall grows linearly with distance, so
//!
//! ```text
//! radius(d) = r0 + k * d          (linear in distance, clamped)
//! ```
//!
//! The same paint mass is spread over an area growing like d^2, so per-area
//! density — what we perceive as opacity — falls off quadratically past a
//! reference distance:
//!
//! ```text
//! opacity(d) = o_max * flow * (d_ref / d)^2   for d > d_ref, else plateau
//! ```
//!
//! Incidence angle: when the spray axis is not perpendicular to the wall,
//! the cone's cross-section is an ellipse stretched along the tilt
//! direction (major = r / cos, minor = r) and paint piles up on the near
//! side — the side opposite the direction the ray leans. `Footprint`
//! carries that geometry per dab.
//!
//! Real mode also thins deposition at speed, degrades output (かすれ)
//! while `shake_level` is low, and feeds a density field that spawns drips.
//! Casual mode ignores all of it: a preset is a fixed, forgiving stamp.

use crate::canvas::{Dab, Drip};
use crate::ffi::{BrushPresetF, SprayCapF};
use crate::math::Vec2;
use std::collections::HashMap;

/// Which spray experience the engine produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SprayMode {
    /// カジュアルライト: named brush presets with fixed, forgiving output.
    Casual,
    /// リアルライト: physical spray simulation (caps, distance, angle,
    /// speed, shake, drips).
    Real,
}

/// A casual-mode brush preset: the dab is a fixed soft disc, independent of
/// distance, angle, speed and shake.
#[derive(Clone, Copy, Debug)]
pub struct BrushPreset {
    pub id: BrushPresetF,
    /// Fixed dab radius, meters.
    pub radius: f32,
    /// Fixed per-dab alpha.
    pub opacity: f32,
    /// Edge softness: 0 = crisp (marker), 1 = very soft halo.
    pub softness: f32,
    /// Dab spacing as a fraction of radius.
    pub spacing_ratio: f32,
}

/// Supported casual presets (`Engine` default: the first).
pub const PRESETS: &[BrushPreset] = &[
    BrushPreset {
        id: BrushPresetF::Broad,
        radius: 0.050,
        opacity: 0.90,
        softness: 0.45,
        spacing_ratio: 0.33,
    },
    BrushPreset {
        id: BrushPresetF::FinePen,
        radius: 0.012,
        opacity: 0.95,
        softness: 0.15,
        spacing_ratio: 0.35,
    },
    BrushPreset {
        id: BrushPresetF::Marker,
        radius: 0.035,
        opacity: 1.00,
        softness: 0.06,
        spacing_ratio: 0.30,
    },
    BrushPreset {
        id: BrushPresetF::Soft,
        radius: 0.090,
        opacity: 0.35,
        softness: 0.90,
        spacing_ratio: 0.50,
    },
];

/// A real-mode spray cap ("ノズル"). Sets the spray cone opening, the paint
/// flow and the contact radius — the same role as fat/skinny caps on a can.
#[derive(Clone, Copy, Debug)]
pub struct Cap {
    pub id: SprayCapF,
    /// tan(cone half-angle): footprint radius growth per meter of distance.
    pub cone_tan: f32,
    /// Paint flow multiplier (1.0 = standard). Scales opacity.
    pub flow: f32,
    /// Footprint radius at zero distance, meters.
    pub r_nozzle: f32,
    /// Hard cap on footprint radius for this nozzle.
    pub radius_max: f32,
}

/// Real-mode caps, widest first.
pub const CAPS: &[Cap] = &[
    Cap {
        id: SprayCapF::Fat,
        cone_tan: 0.38,
        flow: 1.6,
        r_nozzle: 0.030,
        radius_max: 0.55,
    },
    Cap {
        id: SprayCapF::Standard,
        cone_tan: 0.23,
        flow: 1.0,
        r_nozzle: 0.015,
        radius_max: 0.35,
    },
    Cap {
        id: SprayCapF::Skinny,
        cone_tan: 0.12,
        flow: 0.6,
        r_nozzle: 0.008,
        radius_max: 0.20,
    },
    Cap {
        id: SprayCapF::Needle,
        cone_tan: 0.05,
        flow: 0.3,
        r_nozzle: 0.004,
        radius_max: 0.10,
    },
];

/// The preset table entry for `id`.
pub fn preset(id: BrushPresetF) -> &'static BrushPreset {
    PRESETS
        .iter()
        .find(|p| p.id == id)
        .expect("every brush preset is in PRESETS")
}

/// The cap table entry for `id`.
pub fn cap(id: SprayCapF) -> &'static Cap {
    CAPS.iter()
        .find(|c| c.id == id)
        .expect("every spray cap is in CAPS")
}

/// Largest `Cap::flow`, used to normalize the haptics flow output.
pub const MAX_CAP_FLOW: f32 = 1.6;

/// Incidence cosine clamp: beyond ~78 deg the ellipse stops stretching.
const MIN_COS_INCIDENCE: f32 = 0.2;
/// Absolute ceiling on the ellipse major axis, meters.
const MAX_MAJOR_M: f32 = 0.9;
/// Below this wall-space speed paint deposits at full strength.
const FULL_DEPOSITION_SPEED: f32 = 0.25;
/// Fastest strokes still lay down this fraction of paint.
const MIN_DEPOSITION: f32 = 0.2;
/// While held still, a real can keeps wetting the spot: re-stamp interval.
const DWELL_INTERVAL_S: f32 = 0.15;
/// Accumulated per-cell opacity that spawns a drip. Crossing each further
/// multiple spawns another, longer streak.
pub const DRIP_DENSITY_THRESHOLD: f32 = 4.0;
/// Density accumulation cell size, meters (wall-local).
const DENSITY_CELL_M: f32 = 0.04;

#[derive(Clone, Copy, Debug)]
pub struct SprayModel {
    /// Disc radius at zero distance (the nozzle diameter), meters.
    pub r_nozzle: f32,
    /// Radius growth per meter of distance — tan of the cone half-angle.
    pub growth_per_m: f32,
    pub radius_min: f32,
    pub radius_max: f32,
    /// Distance at which opacity starts to roll off (near-contact plateau).
    pub d_ref: f32,
    pub opacity_max: f32,
    pub opacity_min: f32,
    /// Dab spacing as a fraction of radius (overlap factor).
    pub spacing_ratio: f32,
    /// Absolute minimum spacing, meters.
    pub spacing_min: f32,
    /// Cap flow multiplier applied to opacity.
    pub flow: f32,
}

impl Default for SprayModel {
    fn default() -> Self {
        Self {
            // ~1.5 cm radius at contact; cone half-angle ~13 deg.
            r_nozzle: 0.015,
            growth_per_m: 0.23,
            radius_min: 0.010,
            radius_max: 0.35,
            // Full-strength spray within ~35 cm of the wall.
            d_ref: 0.35,
            opacity_max: 0.85,
            opacity_min: 0.04,
            // Dabs overlap ~3x their radius for smooth coverage.
            spacing_ratio: 0.33,
            spacing_min: 0.003,
            flow: 1.0,
        }
    }
}

impl SprayModel {
    /// Build the distance model for a real-mode cap.
    pub fn for_cap(cap: &Cap) -> Self {
        Self {
            r_nozzle: cap.r_nozzle,
            growth_per_m: cap.cone_tan,
            radius_min: cap.r_nozzle * 0.5,
            radius_max: cap.radius_max,
            flow: cap.flow,
            ..Default::default()
        }
    }

    /// Spray disc radius at `d` meters from the wall. Monotonic non-decreasing
    /// in `d`, clamped to [radius_min, radius_max]. Negative distances behave
    /// like contact.
    pub fn radius(&self, d: f32) -> f32 {
        (self.r_nozzle + self.growth_per_m * d.max(0.0))
            .clamp(self.radius_min, self.radius_max)
    }

    /// Per-dab alpha at `d` meters (cap flow included). Monotonic
    /// non-increasing in `d`, clamped to [opacity_min, opacity_max].
    pub fn opacity(&self, d: f32) -> f32 {
        let d = d.max(self.d_ref); // plateau inside d_ref
        let o = self.opacity_max * self.flow * (self.d_ref / d).powi(2);
        o.clamp(self.opacity_min, self.opacity_max)
    }

    /// Distance between successive dabs at the given radius.
    pub fn spacing(&self, radius: f32) -> f32 {
        (self.spacing_ratio * radius).max(self.spacing_min)
    }
}

/// Per-tick context the engine computes from the camera ray and wall frame.
#[derive(Clone, Copy, Debug)]
pub struct SprayContext {
    /// Ray distance to the wall (paint travel distance), meters.
    pub distance: f32,
    /// Cosine of the incidence angle: |ray_dir · wall_normal|. 1 = head-on.
    pub cos_incidence: f32,
    /// Ray direction projected onto the wall plane, in wall-local coords.
    /// Its direction is the tilt the ellipse stretches along; zero when
    /// spraying head-on.
    pub tilt: Vec2,
    /// Frame delta time, seconds — drives speed thinning and dwell stamping.
    pub dt: f32,
    /// Current can-shake level 0..1 (1 = freshly shaken).
    pub shake_level: f32,
}

/// Geometric footprint of one dab on the wall, before deposition factors.
#[derive(Clone, Copy, Debug)]
pub struct Footprint {
    pub radius_major: f32,
    pub radius_minor: f32,
    /// Major-axis angle in wall-local radians (the tilt direction).
    pub rotation: f32,
    /// Unit vector toward the denser (near) side; zero when head-on.
    pub gradient: Vec2,
    /// Base alpha before speed/shake factors.
    pub opacity: f32,
    /// Edge softness 0..1.
    pub softness: f32,
}

/// State for one in-flight stroke: positions dabs along the path so spacing
/// stays uniform across frame ticks and direction changes.
pub struct StrokePainter {
    mode: SprayMode,
    /// Real-mode distance model (from the selected cap).
    model: SprayModel,
    /// Casual-mode preset (used when `mode == Casual`).
    preset: &'static BrushPreset,
    last_uv: Option<Vec2>,
    /// Distance travelled since the last emitted dab (< spacing).
    carry: f32,
    /// Stationary-spray accumulator for dwell stamping, seconds.
    dwell: f32,
    /// Deterministic per-stroke noise source (shake speckle).
    rng: Rng,
    /// Base (pre-noise) per-dab alpha of the last `advance` — the shell shows
    /// this as the spray density for haptics.
    last_density: f32,
    /// Normalized paint flow 0..1 of the last `advance` — haptics intensity.
    last_flow: f32,
}

impl StrokePainter {
    pub fn new(mode: SprayMode, cap: &Cap, preset: &'static BrushPreset, seed: u64) -> Self {
        Self {
            mode,
            model: SprayModel::for_cap(cap),
            preset,
            last_uv: None,
            carry: 0.0,
            dwell: 0.0,
            rng: Rng::new(seed),
            last_density: 0.0,
            last_flow: 0.0,
        }
    }

    /// Normalized paint flow of the last frame (0..1) for haptics.
    pub fn flow(&self) -> f32 {
        self.last_flow
    }

    /// Base per-dab alpha of the last frame (0..1) for haptics.
    pub fn density(&self) -> f32 {
        self.last_density
    }

    /// Dab geometry for the current context. Casual: the fixed preset disc.
    /// Real: distance circle stretched into an ellipse by incidence angle.
    pub fn footprint(&self, ctx: &SprayContext) -> Footprint {
        if self.mode == SprayMode::Casual {
            let p = self.preset;
            return Footprint {
                radius_major: p.radius,
                radius_minor: p.radius,
                rotation: 0.0,
                gradient: Vec2::new(0.0, 0.0),
                opacity: p.opacity,
                softness: p.softness,
            };
        }
        let r = self.model.radius(ctx.distance);
        let cos = ctx.cos_incidence.clamp(MIN_COS_INCIDENCE, 1.0);
        let major = (r / cos).clamp(r, MAX_MAJOR_M);
        let tilt_len = ctx.tilt.len();
        let (rotation, gradient) = if tilt_len > 1e-3 {
            let dir = Vec2::new(ctx.tilt.x / tilt_len, ctx.tilt.y / tilt_len);
            // Near side is opposite the tilt direction: paint lands denser
            // where the cone meets the wall more squarely.
            (dir.y.atan2(dir.x), Vec2::new(-dir.x, -dir.y))
        } else {
            (0.0, Vec2::new(0.0, 0.0))
        };
        Footprint {
            radius_major: major,
            radius_minor: r,
            rotation,
            gradient,
            opacity: self.model.opacity(ctx.distance),
            // Far mist is softer than a contact jet.
            softness: (0.2 + 0.3 * ctx.distance).clamp(0.2, 0.9),
        }
    }

    /// Advance the spray point to `uv`; returns the dabs emitted along the
    /// segment since the previous call. The first call emits a dab at `uv`
    /// itself (the "can just touched the wall" stamp). Holding the nozzle
    /// still keeps depositing paint on a dwell interval — that is what lets
    /// a spot saturate and drip.
    pub fn advance(&mut self, uv: Vec2, ctx: &SprayContext, out: &mut Vec<Dab>) {
        let fp = self.footprint(ctx);
        self.last_density = fp.opacity;
        self.last_flow = match self.mode {
            SprayMode::Casual => self.preset.opacity,
            SprayMode::Real => {
                (self.model.flow / MAX_CAP_FLOW) * (fp.opacity / self.model.opacity_max)
            }
        }
        .clamp(0.0, 1.0);
        let spacing = match self.mode {
            SprayMode::Casual => (self.preset.spacing_ratio * self.preset.radius).max(0.003),
            SprayMode::Real => self.model.spacing(fp.radius_minor),
        };

        let Some(from) = self.last_uv else {
            self.last_uv = Some(uv);
            self.push(out, uv, &fp, 1.0, ctx);
            return;
        };

        let delta = uv - from;
        let seg_len = delta.len();
        self.last_uv = Some(uv);
        if seg_len < 1e-4 {
            // Stationary: keep wetting the spot like a real can would.
            self.dwell += ctx.dt;
            if self.dwell >= DWELL_INTERVAL_S {
                self.dwell = 0.0;
                self.push(out, uv, &fp, 1.0, ctx);
            }
            return;
        }
        self.dwell = 0.0;

        let speed = seg_len / ctx.dt.max(1e-3);
        let speed_f = self.speed_factor(speed);
        let dir = Vec2::new(delta.x / seg_len, delta.y / seg_len);

        // Distance into this segment of the next dab. If we are already
        // "overdue" (spacing shrank because distance changed), emit at once.
        let mut x = if self.carry >= spacing {
            0.0
        } else {
            spacing - self.carry
        };
        if x <= seg_len {
            let mut last_x = x;
            while x <= seg_len {
                self.push(out, from + dir * x, &fp, speed_f, ctx);
                last_x = x;
                x += spacing;
            }
            self.carry = seg_len - last_x;
        } else {
            self.carry += seg_len;
        }
    }

    /// Deposition loss from hand speed: below `FULL_DEPOSITION_SPEED` the
    /// paint lands at full strength; past it, alpha falls off like 1/speed
    /// (to a floor) — the "lines go thin when you whip" effect.
    fn speed_factor(&self, speed: f32) -> f32 {
        if self.mode == SprayMode::Casual {
            return 1.0;
        }
        (FULL_DEPOSITION_SPEED / speed.max(FULL_DEPOSITION_SPEED))
            .clamp(MIN_DEPOSITION, 1.0)
    }

    /// かすれ: low shake level speckles/drops out the spray (Real only).
    /// At level 1 output is clean; toward 0 dabs get patchy and faint.
    fn shake_factor(&mut self, level: f32) -> f32 {
        let level = level.clamp(0.0, 1.0);
        if level >= 1.0 {
            return 1.0;
        }
        let speckle = 0.1 + 0.5 * self.rng.f32();
        speckle + (1.0 - speckle) * level
    }

    fn push(&mut self, out: &mut Vec<Dab>, uv: Vec2, fp: &Footprint, speed_f: f32, ctx: &SprayContext) {
        let shake_f = match self.mode {
            SprayMode::Casual => 1.0,
            SprayMode::Real => self.shake_factor(ctx.shake_level),
        };
        out.push(Dab {
            uv,
            radius_major: fp.radius_major,
            radius_minor: fp.radius_minor,
            rotation: fp.rotation,
            gradient: fp.gradient,
            opacity: (fp.opacity * speed_f * shake_f).clamp(0.0, 1.0),
            softness: fp.softness,
        });
    }
}

/// Accumulated paint density over the canvas, on a coarse wall-local grid.
/// When a cell's total deposited alpha crosses a multiple of
/// `DRIP_DENSITY_THRESHOLD` the paint runs — the engine spawns a `Drip`.
/// Real mode only; casual presets never saturate.
#[derive(Default)]
pub struct DensityField {
    cells: HashMap<(i32, i32), f32>,
}

impl DensityField {
    pub fn reset(&mut self) {
        self.cells.clear();
    }

    /// Deposit a dab. Returns the cell's new accumulated density only when a
    /// drip threshold multiple is crossed (call `spawn_drip` then).
    pub fn accumulate(&mut self, dab: &Dab) -> Option<f32> {
        let key = (
            (dab.uv.x / DENSITY_CELL_M).floor() as i32,
            (dab.uv.y / DENSITY_CELL_M).floor() as i32,
        );
        let entry = self.cells.entry(key).or_insert(0.0);
        let before = *entry;
        *entry += dab.opacity;
        let crossed = (*entry / DRIP_DENSITY_THRESHOLD).floor() as u32
            > (before / DRIP_DENSITY_THRESHOLD).floor() as u32;
        crossed.then_some(*entry)
    }
}

/// Build a drip streak at `uv` for a cell that just saturated. Runs straight
/// down wall-local -v; length grows with the density overage.
pub fn spawn_drip(uv: Vec2, density: f32, rng: &mut Rng) -> Drip {
    let over = density - DRIP_DENSITY_THRESHOLD;
    Drip {
        uv: Vec2::new(uv.x + (rng.f32() - 0.5) * 0.02, uv.y),
        len: (0.08 + 0.06 * over).clamp(0.08, 0.45),
        width: 0.005 + rng.f32() * 0.004,
        opacity: 0.9,
    }
}

/// Small deterministic RNG (xorshift64*) — speckle noise and drip jitter.
/// Deterministic so tests and replays see the same speckle pattern.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform float in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / 16_777_216.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STANDARD: &Cap = &CAPS[1]; // Standard — matches the old default model

    fn ctx(distance: f32) -> SprayContext {
        SprayContext {
            distance,
            cos_incidence: 1.0,
            tilt: Vec2::new(0.0, 0.0),
            dt: 1.0 / 60.0,
            shake_level: 1.0,
        }
    }

    fn painter() -> StrokePainter {
        StrokePainter::new(SprayMode::Real, STANDARD, &PRESETS[0], 42)
    }

    #[test]
    fn radius_monotonic_and_clamped() {
        let m = SprayModel::default();
        let mut prev = 0.0;
        for i in 0..=400 {
            let d = i as f32 * 0.01; // 0..4 m
            let r = m.radius(d);
            assert!(r >= prev, "radius decreased at d={d}");
            assert!(r >= m.radius_min && r <= m.radius_max);
            prev = r;
        }
        // Hard clamps at extremes.
        assert_eq!(m.radius(-1.0), m.radius(0.0));
        assert_eq!(m.radius(100.0), m.radius_max);
    }

    #[test]
    fn opacity_monotonic_and_clamped() {
        let m = SprayModel::default();
        let mut prev = f32::INFINITY;
        for i in 0..=400 {
            let d = i as f32 * 0.01; // 0..4 m
            let o = m.opacity(d);
            assert!(o <= prev, "opacity increased at d={d}");
            assert!(o >= m.opacity_min && o <= m.opacity_max);
            prev = o;
        }
        assert_eq!(m.opacity(0.0), m.opacity_max);
        assert_eq!(m.opacity(50.0), m.opacity_min);
        // Plateau inside d_ref.
        assert_eq!(m.opacity(m.d_ref * 0.5), m.opacity_max);
    }

    #[test]
    fn dab_spacing_is_uniform() {
        let mut p = painter();
        let mut dabs = Vec::new();
        // Straight 1 m stroke at fixed distance, fed in 5 cm frame steps.
        let d = 0.5;
        for i in 0..=20 {
            p.advance(Vec2::new(i as f32 * 0.05, 0.0), &ctx(d), &mut dabs);
        }
        let spacing = SprayModel::for_cap(STANDARD).spacing(SprayModel::for_cap(STANDARD).radius(d));
        // First dab at start; then evenly spaced.
        assert!(dabs.len() >= 2);
        for w in dabs.windows(2) {
            let gap = (w[1].uv - w[0].uv).len();
            assert!((gap - spacing).abs() < 1e-5, "gap {gap} != spacing {spacing}");
        }
        // Coverage ~ length / spacing.
        let expected = 1.0 / spacing;
        assert!((dabs.len() as f32 - expected).abs() < 2.0);
        // All dabs carry the same minor radius for fixed distance.
        let r = SprayModel::for_cap(STANDARD).radius(d);
        assert!(dabs.iter().all(|dab| (dab.radius_minor - r).abs() < 1e-6));
    }

    #[test]
    fn dab_spacing_survives_direction_changes() {
        let mut p = painter();
        let mut dabs = Vec::new();
        let d = 0.4;
        // L-shaped path: spacing should stay uniform around the corner.
        let path = [
            Vec2::new(0.0, 0.0),
            Vec2::new(0.4, 0.0),
            Vec2::new(0.4, 0.4),
        ];
        for w in path.windows(2) {
            p.advance(w[1], &ctx(d), &mut dabs);
        }
        let spacing = SprayModel::for_cap(STANDARD).spacing(SprayModel::for_cap(STANDARD).radius(d));
        for w in dabs.windows(2) {
            let gap = (w[1].uv - w[0].uv).len();
            // Gaps stay <= spacing (corner dab may compress the gap slightly).
            assert!(gap <= spacing + 1e-5 && gap > spacing * 0.2);
        }
    }

    #[test]
    fn ellipse_major_grows_with_incidence() {
        let p = painter();
        let c = ctx(0.4);
        let mut prev_major = 0.0;
        for &cos in &[1.0f32, 0.8, 0.6, 0.4, 0.25] {
            let mut angled = c;
            angled.cos_incidence = cos;
            angled.tilt = Vec2::new((1.0 - cos * cos).sqrt(), 0.0);
            let fp = p.footprint(&angled);
            assert!(fp.radius_major > prev_major, "major did not grow at cos={cos}");
            prev_major = fp.radius_major;
        }
        // Head-on: a circle.
        let head_on = p.footprint(&c);
        assert!((head_on.radius_major - head_on.radius_minor).abs() < 1e-6);
        assert_eq!(head_on.gradient.len(), 0.0);
    }

    #[test]
    fn gradient_points_to_near_side() {
        let p = painter();
        // Tilted to wall-local +u: the dense (near) side is -u.
        let mut c = ctx(0.4);
        c.cos_incidence = 0.5;
        c.tilt = Vec2::new(0.866, 0.0);
        let fp = p.footprint(&c);
        assert!((fp.gradient.len() - 1.0).abs() < 1e-4);
        assert!(fp.gradient.x < 0.0 && fp.gradient.y.abs() < 1e-4);
        // Major axis lies along the tilt (+u): rotation ~ 0.
        assert!(fp.rotation.abs() < 0.05);
        // Tilted up (+v): dense side is -v, major axis is vertical (~90 deg).
        c.tilt = Vec2::new(0.0, 0.866);
        let fp = p.footprint(&c);
        assert!(fp.gradient.y < 0.0 && fp.gradient.x.abs() < 1e-4);
        assert!((fp.rotation - std::f32::consts::FRAC_PI_2).abs() < 0.05);
    }

    #[test]
    fn speed_thins_deposition() {
        let mut slow = painter();
        let mut fast = painter();
        let mut slow_dabs = Vec::new();
        let mut fast_dabs = Vec::new();
        let d = 0.4;
        // Slow pass: 5 cm covered over 0.5 s = 0.1 m/s, below full-deposition.
        let mut slow_ctx = ctx(d);
        slow_ctx.dt = 0.5;
        slow.advance(Vec2::new(0.0, 0.0), &slow_ctx, &mut slow_dabs);
        slow.advance(Vec2::new(0.05, 0.0), &slow_ctx, &mut slow_dabs);
        // Fast pass: 40 cm in one 16 ms frame ≈ 24 m/s — well past thinning.
        fast.advance(Vec2::new(0.0, 0.0), &ctx(d), &mut fast_dabs);
        fast.advance(Vec2::new(0.40, 0.0), &ctx(d), &mut fast_dabs);
        let slow_op: f32 = slow_dabs[1..].iter().map(|d| d.opacity).sum::<f32>()
            / (slow_dabs.len() - 1) as f32;
        let fast_op: f32 =
            fast_dabs[1..].iter().map(|d| d.opacity).sum::<f32>() / (fast_dabs.len() - 1) as f32;
        assert!(
            fast_op < slow_op * 0.7,
            "fast stroke should be much thinner: slow={slow_op} fast={fast_op}"
        );
        // Casual mode ignores speed entirely.
        let mut casual = StrokePainter::new(SprayMode::Casual, STANDARD, &PRESETS[0], 42);
        let mut dabs = Vec::new();
        casual.advance(Vec2::new(0.0, 0.0), &ctx(d), &mut dabs);
        casual.advance(Vec2::new(0.40, 0.0), &ctx(d), &mut dabs);
        assert!(dabs.iter().all(|d| (d.opacity - PRESETS[0].opacity).abs() < 1e-6));
    }

    #[test]
    fn every_id_has_one_table_entry() {
        use BrushPresetF::*;
        use SprayCapF::*;
        for id in [Broad, FinePen, Marker, Soft] {
            assert_eq!(preset(id).id, id);
            assert_eq!(PRESETS.iter().filter(|p| p.id == id).count(), 1);
        }
        for id in [Fat, Standard, Skinny, Needle] {
            assert_eq!(cap(id).id, id);
            assert_eq!(CAPS.iter().filter(|c| c.id == id).count(), 1);
        }
        assert_eq!((PRESETS.len(), CAPS.len()), (4, 4));
    }

    #[test]
    fn casual_presets_are_stable() {
        // Distance, angle, speed and shake must not move casual output.
        for (i, preset) in PRESETS.iter().enumerate() {
            let mut p = StrokePainter::new(SprayMode::Casual, STANDARD, preset, 7);
            let mut dabs = Vec::new();
            for (d, cos, dt, shake) in [
                (0.1f32, 1.0f32, 1.0 / 60.0, 1.0f32),
                (2.0, 0.4, 0.005, 0.0),
                (0.5, 0.8, 0.2, 0.2),
            ] {
                let mut c = ctx(d);
                c.cos_incidence = cos;
                c.tilt = Vec2::new(0.6, 0.0);
                c.dt = dt;
                c.shake_level = shake;
                p.advance(Vec2::new(0.05 * i as f32, 0.0), &c, &mut dabs);
            }
            assert!(!dabs.is_empty(), "preset {:?} emitted nothing", preset.id);
            for dab in &dabs {
                assert!((dab.radius_major - preset.radius).abs() < 1e-6);
                assert!((dab.radius_minor - preset.radius).abs() < 1e-6);
                assert!((dab.opacity - preset.opacity).abs() < 1e-6);
                assert!((dab.softness - preset.softness).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn shake_speckle_real_mode_only() {
        let d = 0.4;
        // Real, fully shaken: opacity clean.
        let mut real = painter();
        let mut c = ctx(d);
        let mut dabs = Vec::new();
        real.advance(Vec2::new(0.0, 0.0), &c, &mut dabs);
        let clean = dabs[0].opacity;
        // Real, unshaken: repeated dwell dabs speckle below the clean value.
        c.shake_level = 0.0;
        let mut speckled = Vec::new();
        for _ in 0..60 {
            real.advance(Vec2::new(0.0, 0.0), &c, &mut speckled);
        }
        assert!(speckled.len() > 2, "dwell should keep stamping");
        let mean: f32 = speckled.iter().map(|d| d.opacity).sum::<f32>() / speckled.len() as f32;
        assert!(mean < clean * 0.7, "low shake should speckle: mean={mean} clean={clean}");
        // Casual ignores shake entirely.
        let mut casual = StrokePainter::new(SprayMode::Casual, STANDARD, &PRESETS[0], 7);
        let mut dabs = Vec::new();
        casual.advance(Vec2::new(0.0, 0.0), &c, &mut dabs);
        assert!((dabs[0].opacity - PRESETS[0].opacity).abs() < 1e-6);
    }

    #[test]
    fn drips_only_above_threshold() {
        let mut field = DensityField::default();
        let dab = Dab {
            uv: Vec2::new(0.0, 0.0),
            radius_major: 0.05,
            radius_minor: 0.05,
            rotation: 0.0,
            gradient: Vec2::new(0.0, 0.0),
            opacity: 0.85,
            softness: 0.5,
        };
        // Under the threshold: nothing.
        for _ in 0..3 {
            assert!(field.accumulate(&dab).is_none());
        }
        // Crossing 4.0 spawns; each further multiple spawns again.
        let mut hits = 0;
        for _ in 0..20 {
            if field.accumulate(&dab).is_some() {
                hits += 1;
            }
        }
        assert!(hits >= 3, "expected repeated triggers, got {hits}");
        // A faint dab far away never reaches the threshold.
        let mut field2 = DensityField::default();
        let faint = Dab { opacity: 0.05, ..dab };
        for _ in 0..30 {
            assert!(field2.accumulate(&faint).is_none());
        }
    }

    #[test]
    fn drip_geometry_runs_down() {
        let mut rng = Rng::new(9);
        let d = spawn_drip(Vec2::new(0.5, 1.0), 6.0, &mut rng);
        assert!(d.len > 0.08 && d.len <= 0.45);
        assert!(d.width > 0.0 && d.opacity <= 1.0);
        // More overage -> longer streak.
        let d2 = spawn_drip(Vec2::new(0.5, 1.0), 12.0, &mut rng);
        assert!(d2.len >= d.len);
    }
}
