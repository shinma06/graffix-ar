//! UniFFI boundary types. Everything crossing into Swift is a plain record or
//! enum defined here; the internal math types convert to/from these.

/// 2D vector (meters in wall-local space unless noted otherwise).
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct Vec2F {
    pub x: f32,
    pub y: f32,
}

/// 3D vector in world space, meters.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct Vec3F {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Straight RGBA color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ColorF {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// One AR plane observation, delivered by the shell on add/update.
///
/// `center` is a point on the plane, `normal` its facing direction, `axis_u`
/// the wall's local right direction (used to orient local coordinates), and
/// the half extents bound the detected rectangle along (u, v).
#[derive(Clone, Debug, uniffi::Record)]
pub struct PlaneObservationF {
    pub id: u64,
    pub center: Vec3F,
    pub normal: Vec3F,
    pub axis_u: Vec3F,
    pub half_extent_u: f32,
    pub half_extent_v: f32,
}

/// Camera tracking quality, mapped from ARKit's tracking state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TrackingStateF {
    Normal,
    Limited,
    NotAvailable,
}

/// Spray mode: カジュアルライト (forgiving presets) or リアルライト
/// (physical spray simulation).
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SprayModeF {
    Casual,
    Real,
}

/// Casual-mode brush preset (stable identifier; display labels live in the
/// shell).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BrushPresetF {
    /// Wide, fairly opaque soft disc.
    Broad,
    /// Thin, crisp line.
    FinePen,
    /// Medium, fully opaque, hard edge.
    Marker,
    /// Very wide, faint, very soft.
    Soft,
}

/// Real-mode spray cap (stable identifier), widest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum SprayCapF {
    Fat,
    Standard,
    Skinny,
    Needle,
}

/// Spray paint color (stable identifier). Saved strokes store the palette
/// index, not this id; the core maps between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum SprayColorF {
    White,
    Black,
    Silver,
    Red,
    Orange,
    Yellow,
    Green,
    Teal,
    Blue,
    Purple,
    Pink,
    Brown,
}

/// Everything the engine needs for one frame. The shell collects plane
/// add/update/remove events, the camera pose, and the spray-button state and
/// calls `Engine::tick` once per rendered frame.
#[derive(Clone, Debug, uniffi::Record)]
pub struct FrameInputF {
    /// Camera world position.
    pub camera_position: Vec3F,
    /// Center-of-view ray (the "spray nozzle"): origin + unit direction.
    /// Normally the camera position and forward vector.
    pub ray_origin: Vec3F,
    pub ray_dir: Vec3F,
    /// Plane anchors added or updated since the last tick.
    pub plane_upserts: Vec<PlaneObservationF>,
    /// Plane anchor ids removed since the last tick.
    pub plane_removals: Vec<u64>,
    /// Scene-depth sample at screen center in meters (LiDAR devices only).
    pub lidar_center_depth_m: Option<f32>,
    pub tracking: TrackingStateF,
    /// True while the user holds the spray button.
    pub spraying: bool,
    /// Frame delta time, seconds. Drives hand-speed thinning, dwell
    /// stamping and can-shake decay. 0 is treated as ~1/60 s.
    pub dt_seconds: f32,
}

/// What the engine decided during one tick.
#[derive(Clone, Debug, uniffi::Record)]
pub struct FrameOutputF {
    /// Wall currently under the center ray (lock candidate).
    pub aimed_wall_id: Option<u64>,
    /// Locked wall id, if any.
    pub locked_wall_id: Option<u64>,
    /// Smoothed camera->wall distance along the view ray.
    pub distance_m: Option<f32>,
    /// 0..1 reliability of `distance_m`.
    pub distance_reliability: f32,
    /// Distance estimate fused a LiDAR depth sample this frame.
    pub distance_lidar_fused: bool,
    /// Spray point in the locked wall's local coordinates this frame.
    pub spray_uv: Option<Vec2F>,
    /// Number of dabs appended during this tick.
    pub dabs_added: u32,
    /// Canvas changed since the last `canvas_snapshot` worth re-rendering.
    pub canvas_dirty: bool,
    /// A stroke is currently being painted.
    pub stroke_active: bool,
    /// Core spray truth for haptics/UI, every frame.
    pub spray: SprayStateF,
}

/// What the spray engine is doing this frame — the shell drives haptics
/// (and the shake hint) purely from this, never from its own guesses.
#[derive(Clone, Copy, Debug, uniffi::Record)]
pub struct SprayStateF {
    /// Paint is actually leaving the nozzle right now.
    pub spraying: bool,
    /// Active spray mode.
    pub mode: SprayModeF,
    /// Normalized paint flow 0..1 — haptics intensity.
    pub flow: f32,
    /// Base per-dab alpha 0..1 — haptics sharpness.
    pub density: f32,
    /// Can-shake level 0..1 (1 = freshly shaken). Low in Real mode = かすれ.
    pub shake_level: f32,
}

/// One spray stamp (v2): an ellipse at (u, v) in wall-local meters with a
/// density gradient toward the near side.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct DabF {
    pub u: f32,
    pub v: f32,
    /// Ellipse semi-axes, meters. `radius_major_m` lies along the spray tilt.
    pub radius_major_m: f32,
    pub radius_minor_m: f32,
    /// Major-axis angle in wall-local radians.
    pub rotation_rad: f32,
    /// Unit vector toward the denser (near) side, wall-local.
    /// (0, 0) when the dab has no directional density falloff.
    pub gradient_u: f32,
    pub gradient_v: f32,
    /// 0..1 per-dab alpha before color application.
    pub opacity: f32,
    /// Edge softness 0..1 (0 = crisp, 1 = very soft halo).
    pub softness: f32,
}

/// A drip streak running down the wall from (u, v) along -v.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct DripF {
    pub u: f32,
    pub v: f32,
    /// Run length downward, meters.
    pub len_m: f32,
    /// Streak width, meters.
    pub width_m: f32,
    pub opacity: f32,
}

/// A continuous stroke in one color.
#[derive(Clone, Debug, uniffi::Record)]
pub struct StrokeF {
    pub palette_index: u8,
    pub color: ColorF,
    pub dabs: Vec<DabF>,
    /// Drip streaks spawned by this stroke's oversaturation.
    pub drips: Vec<DripF>,
}

/// Orthonormal frame the canvas coordinates live in. Strokes are stored
/// relative to this frame so a saved work can be re-anchored later (phase 2+)
/// by supplying a new world pose for the same wall.
#[derive(Clone, Copy, Debug, uniffi::Record)]
pub struct CanvasFrameF {
    pub origin: Vec3F,
    pub axis_u: Vec3F,
    pub axis_v: Vec3F,
    pub normal: Vec3F,
}

/// Raw canvas contents (strokes with resolved colors). The live wall texture
/// comes from `Engine::render_live_canvas` instead.
#[derive(Clone, Debug, uniffi::Record)]
pub struct CanvasSnapshotF {
    pub frame: Option<CanvasFrameF>,
    pub strokes: Vec<StrokeF>,
    pub dab_count: u32,
}

/// The live wall texture from `Engine::render_live_canvas`: transparent where
/// unpainted, laid on the wall plane at `frame`.
#[derive(Clone, Debug, uniffi::Record)]
pub struct LiveCanvasImageF {
    /// Frozen canvas frame the image lies in.
    pub frame: CanvasFrameF,
    pub width: u32,
    pub height: u32,
    /// Premultiplied-alpha sRGB RGBA8, row-major from the top-left. Rows run
    /// from the +v (top) edge down, columns along +u.
    pub rgba: Vec<u8>,
    /// Wall-local rectangle the image covers, meters: left edge, bottom edge
    /// and size.
    pub min_u: f32,
    pub min_v: f32,
    pub width_m: f32,
    pub height_m: f32,
    /// Image density chosen by the core.
    pub pixels_per_meter: f32,
}

/// A supported spray color and the paint color it lays down.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct SprayColorValueF {
    pub id: SprayColorF,
    pub rgba: ColorF,
}

/// Engine errors surfaced across the FFI.
#[derive(Debug, uniffi::Error)]
pub enum CoreError {
    CorruptData(String),
    NoWallLocked,
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoreError::CorruptData(msg) => write!(f, "canvas data is corrupt: {msg}"),
            CoreError::NoWallLocked => write!(f, "no wall is locked"),
        }
    }
}

impl std::error::Error for CoreError {}
