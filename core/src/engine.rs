//! `Engine`: the UniFFI-facing facade the Swift shell drives once per frame.

use std::sync::{Arc, Mutex};

use crate::canvas::{dab_f, drip_f, Canvas, Stroke};
use crate::distance::{DistanceFilter, Tracking};
use crate::ffi::{
    CanvasFrameF, CanvasSnapshotF, CoreError, FrameInputF, FrameOutputF, NamedEntryF,
    PaletteEntryF, SprayModeF, SprayStateF, TrackingStateF, Vec2F,
};
use crate::math::{PlaneFrame, Ray, Vec2, Vec3};
use crate::palette;
use crate::serialize;
use crate::spray::{
    spawn_drip, DensityField, Rng, SprayContext, SprayMode, StrokePainter, CAPS, PRESETS,
};
use crate::wall::WallRegistry;

/// Shake level added by one `shake()` call (~3 shakes to a full can).
const SHAKE_PER_GESTURE: f32 = 0.34;
/// Shake decay time constant, seconds — a shaken can stays mixed ~20 s.
const SHAKE_DECAY_TAU_S: f32 = 8.0;
/// Longest honored frame delta (hitch protection).
const MAX_DT_S: f32 = 0.5;

impl From<SprayModeF> for SprayMode {
    fn from(m: SprayModeF) -> Self {
        match m {
            SprayModeF::Casual => SprayMode::Casual,
            SprayModeF::Real => SprayMode::Real,
        }
    }
}

impl From<SprayMode> for SprayModeF {
    fn from(m: SprayMode) -> Self {
        match m {
            SprayMode::Casual => SprayModeF::Casual,
            SprayMode::Real => SprayModeF::Real,
        }
    }
}

struct EngineState {
    registry: WallRegistry,
    distance: DistanceFilter,
    canvas: Canvas,
    mode: SprayMode,
    cap_index: u8,
    preset_index: u8,
    /// Can-shake level 0..1, decaying per tick. Real mode only.
    shake_level: f32,
    /// Oversaturation tracking that spawns drips (real mode only).
    density: DensityField,
    /// Jitter source for drip geometry.
    rng: Rng,
    /// Per-stroke seed counter for speckle noise.
    stroke_seed: u64,
    aimed: Option<u64>,
    locked: Option<u64>,
    /// Stroke being painted right now (spray button held).
    painter: Option<StrokePainter>,
    active_stroke: Option<Stroke>,
    color_index: u8,
    /// Camera position from the last tick (used to orient the canvas frame).
    last_camera: Vec3,
    /// Canvas revision at the last `canvas_snapshot` call.
    seen_revision: u64,
}

impl EngineState {
    fn end_stroke(&mut self) {
        self.painter = None;
        if let Some(stroke) = self.active_stroke.take() {
            self.canvas.commit_stroke(stroke);
        }
    }
}

#[derive(uniffi::Object)]
pub struct Engine {
    state: Mutex<EngineState>,
}

#[uniffi::export]
impl Engine {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(EngineState {
                registry: WallRegistry::new(),
                distance: DistanceFilter::default(),
                canvas: Canvas::new(),
                mode: SprayMode::Casual,
                cap_index: 1, // スタンダード
                preset_index: 0,
                shake_level: 0.0,
                density: DensityField::default(),
                rng: Rng::new(0x9E37_79B1),
                stroke_seed: 0,
                aimed: None,
                locked: None,
                painter: None,
                active_stroke: None,
                color_index: 0,
                last_camera: Vec3::ZERO,
                seen_revision: 0,
            }),
        })
    }

    /// One frame of input in, one frame of decisions out. The shell calls this
    /// from its AR frame callback.
    pub fn tick(&self, input: FrameInputF) -> FrameOutputF {
        let mut guard = self.state.lock().unwrap();
        let s = &mut *guard;
        s.last_camera = crate::math::v3(input.camera_position);

        let dt = if input.dt_seconds.is_finite() && input.dt_seconds > 0.0 {
            input.dt_seconds.min(MAX_DT_S)
        } else {
            1.0 / 60.0
        };
        // The can un-mixes over time; decay is frame-rate independent.
        s.shake_level *= (-dt / SHAKE_DECAY_TAU_S).exp();
        if s.shake_level < 0.001 {
            s.shake_level = 0.0;
        }

        for obs in &input.plane_upserts {
            s.registry.upsert(obs);
        }
        for id in &input.plane_removals {
            s.registry.remove(*id);
            if s.aimed == Some(*id) {
                s.aimed = None;
            }
            if s.locked == Some(*id) {
                s.end_stroke();
                s.locked = None;
                s.distance.reset();
            }
        }

        let ray = Ray {
            origin: crate::math::v3(input.ray_origin),
            dir: crate::math::v3(input.ray_dir).normalized(),
        };

        // Aim: which wall is under the nozzle right now.
        let aim = s.registry.aim(ray);
        s.aimed = aim.map(|h| h.wall_id);

        // Geometric distance: ray-plane t for the locked wall (unbounded hit)
        // or, when unlocked, the aimed wall, so the UI can show aim distance.
        let hit = match s.locked {
            Some(id) => s.registry.hit_wall(id, ray),
            None => aim,
        };
        let geometric = hit.map(|h| h.t);

        let tracking = match input.tracking {
            TrackingStateF::Normal => Tracking::Normal,
            TrackingStateF::Limited => Tracking::Limited,
            TrackingStateF::NotAvailable => Tracking::NotAvailable,
        };
        let est = s
            .distance
            .update(geometric, input.lidar_center_depth_m, tracking);
        let spray_distance = est.distance.or(geometric);

        // Spray edges: stroke begins when `spraying` rises while locked on a
        // wall and ends when it falls or the wall disappears.
        let mut dabs_added = 0u32;
        let mut spray_uv = None;
        let mut canvas_changed = false;
        if input.spraying && s.locked.is_some() {
            if let (Some(hit), Some(frame)) = (hit, s.canvas.frame()) {
                let uv = frame.to_local(ray.at(hit.t));
                spray_uv = Some(Vec2F::from(uv));
                if s.painter.is_none() {
                    // Mode, cap and preset are snapshot per stroke so a swap
                    // mid-stroke never changes the stroke retroactively.
                    s.stroke_seed += 1;
                    s.painter = Some(StrokePainter::new(
                        s.mode,
                        &CAPS[s.cap_index as usize],
                        &PRESETS[s.preset_index as usize],
                        s.stroke_seed,
                    ));
                    s.active_stroke = Some(Stroke {
                        palette_index: s.color_index,
                        dabs: Vec::new(),
                        drips: Vec::new(),
                    });
                }
                // Incidence geometry: how obliquely the spray cone meets the
                // wall (cos) and which way it leans across it (tilt).
                let cos_incidence = (-ray.dir.dot(frame.normal)).clamp(0.0, 1.0);
                let proj = ray.dir - frame.normal * ray.dir.dot(frame.normal);
                let tilt = Vec2::new(proj.dot(frame.u), proj.dot(frame.v));
                let ctx = SprayContext {
                    distance: spray_distance.unwrap_or(hit.t),
                    cos_incidence,
                    tilt,
                    dt,
                    shake_level: s.shake_level,
                };
                if let (Some(p), Some(stroke)) = (&mut s.painter, &mut s.active_stroke) {
                    let before = stroke.dabs.len();
                    p.advance(uv, &ctx, &mut stroke.dabs);
                    dabs_added = (stroke.dabs.len() - before) as u32;
                    canvas_changed |= dabs_added > 0;
                    // 垂れ: oversaturated cells run down the wall (real mode).
                    if s.mode == SprayMode::Real {
                        for i in before..stroke.dabs.len() {
                            let dab = stroke.dabs[i];
                            if let Some(density) = s.density.accumulate(&dab) {
                                stroke.drips.push(spawn_drip(dab.uv, density, &mut s.rng));
                                canvas_changed = true;
                            }
                        }
                    }
                }
            }
        }
        if canvas_changed {
            s.canvas.bump_dabs();
        }
        if !input.spraying {
            s.end_stroke();
        }

        let spraying = s.painter.is_some();
        FrameOutputF {
            aimed_wall_id: s.aimed,
            locked_wall_id: s.locked,
            distance_m: est.distance,
            distance_reliability: est.reliability,
            distance_lidar_fused: est.fused_lidar,
            spray_uv,
            dabs_added,
            canvas_dirty: s.canvas.revision() != s.seen_revision,
            stroke_active: s.active_stroke.is_some(),
            spray: SprayStateF {
                spraying,
                mode: s.mode.into(),
                flow: s.painter.as_ref().map_or(0.0, |p| p.flow()),
                density: s.painter.as_ref().map_or(0.0, |p| p.density()),
                shake_level: s.shake_level,
            },
        }
    }

    /// Lock the wall currently under the nozzle. Returns the locked id.
    pub fn lock_aimed(&self) -> Option<u64> {
        let mut s = self.state.lock().unwrap();
        let id = s.aimed?;
        self.lock_inner(&mut s, id)
    }

    /// Lock a specific wall by id.
    pub fn lock_wall(&self, id: u64) -> Option<u64> {
        let mut s = self.state.lock().unwrap();
        self.lock_inner(&mut s, id)
    }

    pub fn unlock(&self) {
        let mut s = self.state.lock().unwrap();
        s.end_stroke();
        s.locked = None;
        s.distance.reset();
    }

    /// Switch spray mode (カジュアル / リアル). Applies to the next stroke;
    /// any in-flight stroke ends first.
    pub fn set_mode(&self, mode: SprayModeF) {
        let mut s = self.state.lock().unwrap();
        s.end_stroke();
        s.mode = mode.into();
    }

    pub fn mode(&self) -> SprayModeF {
        self.state.lock().unwrap().mode.into()
    }

    /// Brush presets for casual mode (書き方 picker entries).
    pub fn presets(&self) -> Vec<NamedEntryF> {
        PRESETS
            .iter()
            .enumerate()
            .map(|(i, p)| NamedEntryF {
                index: i as u8,
                name: p.name.to_string(),
            })
            .collect()
    }

    pub fn set_preset_index(&self, index: u8) {
        let mut s = self.state.lock().unwrap();
        s.end_stroke();
        s.preset_index = index % PRESETS.len() as u8;
    }

    pub fn preset_index(&self) -> u8 {
        self.state.lock().unwrap().preset_index
    }

    /// Spray caps for real mode (ノズル picker entries).
    pub fn caps(&self) -> Vec<NamedEntryF> {
        CAPS.iter()
            .enumerate()
            .map(|(i, c)| NamedEntryF {
                index: i as u8,
                name: c.name.to_string(),
            })
            .collect()
    }

    /// Swap the spray cap (real mode). Applies to the next stroke.
    pub fn set_cap_index(&self, index: u8) {
        let mut s = self.state.lock().unwrap();
        s.end_stroke();
        s.cap_index = index % CAPS.len() as u8;
    }

    pub fn cap_index(&self) -> u8 {
        self.state.lock().unwrap().cap_index
    }

    /// 缶を振る: one shake gesture mixes the can a bit more.
    pub fn shake(&self) {
        let mut s = self.state.lock().unwrap();
        s.shake_level = (s.shake_level + SHAKE_PER_GESTURE).min(1.0);
    }

    /// Current shake level 0..1 (also reported in every tick's spray state).
    pub fn shake_level(&self) -> f32 {
        self.state.lock().unwrap().shake_level
    }

    /// Current spray color (palette index). Applies to the next stroke.
    pub fn set_color_index(&self, index: u8) {
        let mut s = self.state.lock().unwrap();
        s.color_index = index % palette::PALETTE.len() as u8;
    }

    pub fn color_index(&self) -> u8 {
        self.state.lock().unwrap().color_index
    }

    pub fn palette(&self) -> Vec<PaletteEntryF> {
        palette::entries()
    }

    pub fn undo(&self) -> bool {
        let mut s = self.state.lock().unwrap();
        if s.active_stroke.is_some() {
            return false;
        }
        s.canvas.undo()
    }

    pub fn redo(&self) -> bool {
        let mut s = self.state.lock().unwrap();
        if s.active_stroke.is_some() {
            return false;
        }
        s.canvas.redo()
    }

    pub fn clear(&self) {
        let mut s = self.state.lock().unwrap();
        s.end_stroke();
        s.canvas.clear();
        s.density.reset();
    }

    pub fn can_undo(&self) -> bool {
        self.state.lock().unwrap().canvas.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.state.lock().unwrap().canvas.can_redo()
    }

    /// Number of walls currently tracked (for UI hints).
    pub fn wall_count(&self) -> u32 {
        self.state.lock().unwrap().registry.len() as u32
    }

    /// Full canvas snapshot for rendering. Marks the canvas clean.
    pub fn canvas_snapshot(&self) -> CanvasSnapshotF {
        let mut s = self.state.lock().unwrap();
        // Include any in-flight stroke so painting appears live.
        let (frame, mut strokes) = s.canvas.snapshot(palette::color);
        if let Some(active) = &s.active_stroke {
            strokes.push(crate::ffi::StrokeF {
                palette_index: active.palette_index,
                color: palette::color(active.palette_index),
                dabs: active.dabs.iter().map(dab_f).collect(),
                drips: active.drips.iter().map(drip_f).collect(),
            });
        }
        s.seen_revision = s.canvas.revision();
        CanvasSnapshotF {
            frame: frame.map(|f| CanvasFrameF {
                origin: f.origin.into(),
                axis_u: f.u.into(),
                axis_v: f.v.into(),
                normal: f.normal.into(),
            }),
            dab_count: strokes.iter().map(|st| st.dabs.len() as u32).sum(),
            strokes,
        }
    }

    /// Serialize the canvas (v1 wire format). Upload-ready for later phases.
    pub fn serialize_canvas(&self) -> Vec<u8> {
        let s = self.state.lock().unwrap();
        let created = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        serialize::encode(&s.canvas, created)
    }

    /// Replace the canvas from serialized bytes.
    pub fn load_canvas(&self, bytes: Vec<u8>) -> Result<(), CoreError> {
        let (frame, strokes, _created) =
            serialize::decode(&bytes).map_err(CoreError::CorruptData)?;
        let mut s = self.state.lock().unwrap();
        s.end_stroke();
        s.canvas.replace(frame, strokes);
        s.density.reset();
        Ok(())
    }
}

impl Engine {
    fn lock_inner(&self, s: &mut EngineState, id: u64) -> Option<u64> {
        let wall = s.registry.get(id)?;
        // Freeze the canvas frame on first lock: drawn geometry is stable in
        // this frame from then on, facing the user and upright.
        let frame = PlaneFrame::facing(wall.plane(), s.last_camera, Vec3::UP);
        s.canvas.set_frame(frame);
        s.locked = Some(id);
        s.distance.reset();
        Some(id)
    }
}
