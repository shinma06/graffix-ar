//! `SavedWork`: read-only view of a saved GFXC work for the works detail
//! screen — scene simulation and timelapse replay, rendered to RGBA images
//! the shell only has to display.

use std::sync::{Arc, Mutex};

use crate::canvas::Stroke;
use crate::ffi::{CoreError, PaletteEntryF};
use crate::palette;
use crate::raster::{Paint, PaintLayer, View};
use crate::scene::{self, Bounds, Framing, Lighting, Material, Scene};
use crate::serialize;
use crate::timelapse::{MarkKind, Timeline};

/// Largest image side the core renders, pixels.
const MAX_IMAGE_SIDE: u32 = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SceneLightingF {
    Day,
    Overcast,
    Dusk,
    Night,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum WallMaterialF {
    Concrete,
    Shutter,
    Brick,
    Plaster,
}

/// One simulated situation.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct SceneConfigF {
    pub lighting: SceneLightingF,
    pub material: WallMaterialF,
    /// Index into `SceneOptionsF::wall_colors` (0 = the material's color).
    pub wall_color_index: u8,
}

/// Virtual camera framing.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Enum)]
pub enum FramingF {
    /// Fit the whole work with a margin.
    FitWork,
    /// Head-on from this many meters (clamped to the supported range).
    ViewingDistance { meters: f32 },
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct LightingOptionF {
    pub lighting: SceneLightingF,
    /// Japanese display label.
    pub name: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MaterialOptionF {
    pub material: WallMaterialF,
    /// Japanese display label.
    pub name: String,
}

/// Everything the scene picker UI offers.
#[derive(Clone, Debug, uniffi::Record)]
pub struct SceneOptionsF {
    pub lightings: Vec<LightingOptionF>,
    pub materials: Vec<MaterialOptionF>,
    /// Wall recolors; index 0 keeps the material color (its swatch is a
    /// neutral stand-in).
    pub wall_colors: Vec<PaletteEntryF>,
    pub min_viewing_distance_m: f32,
    pub max_viewing_distance_m: f32,
    pub default_viewing_distance_m: f32,
    /// Horizontal field of view the distance framing simulates.
    pub camera_hfov_deg: f32,
}

/// An opaque RGBA8 image, row-major from the top-left.
#[derive(Clone, Debug, uniffi::Record)]
pub struct RasterImageF {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Wall area the image covers, meters.
    pub view_width_m: f32,
    pub view_height_m: f32,
}

#[derive(Clone, Copy, Debug, uniffi::Record)]
pub struct SavedWorkInfoF {
    pub created_unix_time: u64,
    pub stroke_count: u32,
    pub dab_count: u32,
    pub drip_count: u32,
    /// Painted area, meters (0 when empty).
    pub width_m: f32,
    pub height_m: f32,
    pub timelapse_duration_ms: u32,
    /// Timelapse uses recorded timing (v3). False = synthesized from order.
    pub has_recorded_timing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct TimelapseProgressF {
    pub completed_strokes: u32,
    pub active_stroke: Option<u32>,
    pub visible_dabs: u32,
    pub visible_drips: u32,
    /// 0..1 of the replay duration.
    pub fraction: f32,
}

impl From<SceneConfigF> for Scene {
    fn from(c: SceneConfigF) -> Self {
        Scene {
            lighting: match c.lighting {
                SceneLightingF::Day => Lighting::Day,
                SceneLightingF::Overcast => Lighting::Overcast,
                SceneLightingF::Dusk => Lighting::Dusk,
                SceneLightingF::Night => Lighting::Night,
            },
            material: match c.material {
                WallMaterialF::Concrete => Material::Concrete,
                WallMaterialF::Shutter => Material::Shutter,
                WallMaterialF::Brick => Material::Brick,
                WallMaterialF::Plaster => Material::Plaster,
            },
            wall_color: c.wall_color_index,
        }
    }
}

impl From<FramingF> for Framing {
    fn from(f: FramingF) -> Self {
        match f {
            FramingF::FitWork => Framing::FitWork,
            FramingF::ViewingDistance { meters } => Framing::Distance(meters),
        }
    }
}

/// Lighting / material / wall color / distance choices for the scene UI.
#[uniffi::export]
pub fn scene_options() -> SceneOptionsF {
    let lighting_f = |l: Lighting| match l {
        Lighting::Day => SceneLightingF::Day,
        Lighting::Overcast => SceneLightingF::Overcast,
        Lighting::Dusk => SceneLightingF::Dusk,
        Lighting::Night => SceneLightingF::Night,
    };
    let material_f = |m: Material| match m {
        Material::Concrete => WallMaterialF::Concrete,
        Material::Shutter => WallMaterialF::Shutter,
        Material::Brick => WallMaterialF::Brick,
        Material::Plaster => WallMaterialF::Plaster,
    };
    SceneOptionsF {
        lightings: scene::LIGHTINGS
            .iter()
            .map(|&(l, name)| LightingOptionF {
                lighting: lighting_f(l),
                name: name.to_string(),
            })
            .collect(),
        materials: scene::MATERIALS
            .iter()
            .map(|&(m, name)| MaterialOptionF {
                material: material_f(m),
                name: name.to_string(),
            })
            .collect(),
        wall_colors: scene::wall_color_entries(),
        min_viewing_distance_m: scene::MIN_VIEW_DISTANCE_M,
        max_viewing_distance_m: scene::MAX_VIEW_DISTANCE_M,
        default_viewing_distance_m: scene::DEFAULT_VIEW_DISTANCE_M,
        camera_hfov_deg: scene::CAMERA_HFOV_DEG,
    }
}

/// Incremental timelapse paint layer: replaying forward only stamps the
/// newly revealed marks.
struct ReplayCache {
    layer: PaintLayer,
    marks_drawn: usize,
}

#[derive(uniffi::Object)]
pub struct SavedWork {
    strokes: Vec<Stroke>,
    created: u64,
    timeline: Timeline,
    bounds: Option<Bounds>,
    replay: Mutex<Option<ReplayCache>>,
}

impl SavedWork {
    fn from_strokes(strokes: Vec<Stroke>, created: u64) -> Self {
        let timeline = Timeline::build(&strokes);
        let bounds = Bounds::of(
            strokes.iter().flat_map(|s| s.dabs.iter()),
            strokes.iter().flat_map(|s| s.drips.iter()),
        );
        Self {
            strokes,
            created,
            timeline,
            bounds,
            replay: Mutex::new(None),
        }
    }

    fn view(&self, framing: Framing, width: u32, height: u32) -> View {
        let w = width.clamp(1, MAX_IMAGE_SIDE);
        let h = height.clamp(1, MAX_IMAGE_SIDE);
        scene::view_for(self.bounds, framing, w, h)
    }

    /// Stamp timeline marks `range` (paint order) into `layer`.
    fn stamp(&self, layer: &mut PaintLayer, range: std::ops::Range<usize>) {
        for m in &self.timeline.marks[range] {
            let stroke = &self.strokes[m.stroke as usize];
            let paint = Paint::from(palette::color(stroke.palette_index));
            match m.kind {
                MarkKind::Dab(i) => layer.stamp_dab(&stroke.dabs[i as usize], paint),
                MarkKind::Drip(i) => layer.stamp_drip(&stroke.drips[i as usize], paint),
            }
        }
    }

    fn image(&self, layer: &PaintLayer, scene: Scene) -> RasterImageF {
        let v = layer.view;
        RasterImageF {
            width: v.width,
            height: v.height,
            rgba: scene::compose(layer, scene, self.bounds),
            view_width_m: v.width_m(),
            view_height_m: v.height_m(),
        }
    }
}

#[uniffi::export]
impl SavedWork {
    /// Decode a saved work (GFXC v1–v3).
    #[uniffi::constructor]
    pub fn new(bytes: Vec<u8>) -> Result<Arc<Self>, CoreError> {
        let (_frame, strokes, created) =
            serialize::decode(&bytes).map_err(CoreError::CorruptData)?;
        Ok(Arc::new(Self::from_strokes(strokes, created)))
    }

    pub fn info(&self) -> SavedWorkInfoF {
        SavedWorkInfoF {
            created_unix_time: self.created,
            stroke_count: self.strokes.len() as u32,
            dab_count: self.strokes.iter().map(|s| s.dabs.len() as u32).sum(),
            drip_count: self.strokes.iter().map(|s| s.drips.len() as u32).sum(),
            width_m: self.bounds.map_or(0.0, |b| b.width()),
            height_m: self.bounds.map_or(0.0, |b| b.height()),
            timelapse_duration_ms: self.timeline.duration_ms,
            has_recorded_timing: self.timeline.recorded,
        }
    }

    /// The finished work in a simulated scene.
    pub fn render_scene(
        &self,
        scene: SceneConfigF,
        framing: FramingF,
        width: u32,
        height: u32,
    ) -> RasterImageF {
        let mut layer = PaintLayer::new(self.view(framing.into(), width, height));
        self.stamp(&mut layer, 0..self.timeline.marks.len());
        self.image(&layer, scene.into())
    }

    pub fn timelapse_progress(&self, t_ms: u32) -> TimelapseProgressF {
        let p = self.timeline.progress(t_ms);
        let d = self.timeline.duration_ms;
        TimelapseProgressF {
            completed_strokes: p.completed_strokes,
            active_stroke: p.active_stroke,
            visible_dabs: p.visible_dabs,
            visible_drips: p.visible_drips,
            fraction: if d == 0 {
                1.0
            } else {
                (t_ms.min(d) as f32) / d as f32
            },
        }
    }

    /// The work as it stood `t_ms` into the replay, framed to fit the whole
    /// work so the picture does not move while it grows.
    pub fn render_timelapse_frame(
        &self,
        t_ms: u32,
        scene: SceneConfigF,
        width: u32,
        height: u32,
    ) -> RasterImageF {
        let view = self.view(Framing::FitWork, width, height);
        let target = self.timeline.progress(t_ms).visible_marks;
        let mut guard = self.replay.lock().unwrap();
        let reusable = guard
            .as_ref()
            .is_some_and(|c| c.layer.view == view && c.marks_drawn <= target);
        if !reusable {
            *guard = Some(ReplayCache {
                layer: PaintLayer::new(view),
                marks_drawn: 0,
            });
        }
        let cache = guard.as_mut().unwrap();
        self.stamp(&mut cache.layer, cache.marks_drawn..target);
        cache.marks_drawn = target;
        self.image(&cache.layer, scene.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{Canvas, Dab, Drip, StrokeTiming};
    use crate::math::Vec2;

    fn work() -> Arc<SavedWork> {
        let mut c = Canvas::new();
        for (k, color) in [(0usize, 3u8), (1, 8)] {
            let n = 12;
            c.commit_stroke(Stroke {
                palette_index: color,
                dabs: (0..n)
                    .map(|i| {
                        Dab::circle(
                            Vec2::new(-0.3 + i as f32 * 0.05, 0.2 * k as f32),
                            0.06,
                            0.9,
                            0.4,
                        )
                    })
                    .collect(),
                drips: vec![Drip {
                    uv: Vec2::new(0.0, 0.2 * k as f32),
                    len: 0.2,
                    width: 0.008,
                    opacity: 0.9,
                }],
                timing: Some(StrokeTiming {
                    start_ms: 2_000 * k as u32,
                    dab_ms: (0..n as u32).map(|i| i * 20).collect(),
                    drip_ms: vec![100],
                }),
            });
        }
        SavedWork::new(serialize::encode(&c, 1_731_650_000)).unwrap()
    }

    fn day_concrete() -> SceneConfigF {
        SceneConfigF {
            lighting: SceneLightingF::Day,
            material: WallMaterialF::Concrete,
            wall_color_index: 0,
        }
    }

    #[test]
    fn corrupt_bytes_are_rejected() {
        assert!(matches!(
            SavedWork::new(b"nope".to_vec()),
            Err(CoreError::CorruptData(_))
        ));
    }

    #[test]
    fn info_summarizes_the_work() {
        let i = work().info();
        assert_eq!((i.stroke_count, i.dab_count, i.drip_count), (2, 24, 2));
        assert_eq!(i.created_unix_time, 1_731_650_000);
        assert!(i.has_recorded_timing);
        assert!((i.width_m - (0.55 + 0.12)).abs() < 1e-4);
        // 220 ms stroke, compressed gap, 220 ms stroke.
        assert_eq!(
            i.timelapse_duration_ms,
            220 + crate::timelapse::MAX_PAUSE_MS + 220
        );
    }

    #[test]
    fn options_cover_every_variant() {
        let o = scene_options();
        assert_eq!(o.lightings.len(), 4);
        assert_eq!(o.materials.len(), 4);
        assert_eq!(o.wall_colors.len(), scene::WALL_COLORS.len());
        assert!(o.min_viewing_distance_m < o.default_viewing_distance_m);
        assert!(o.default_viewing_distance_m < o.max_viewing_distance_m);
    }

    #[test]
    fn render_scene_sizes_and_clamps() {
        let w = work();
        let img = w.render_scene(
            day_concrete(),
            FramingF::ViewingDistance { meters: 3.0 },
            160,
            120,
        );
        assert_eq!(
            (img.width, img.height, img.rgba.len()),
            (160, 120, 160 * 120 * 4)
        );
        assert!((img.view_width_m - 2.0 * 3.0 * 30f32.to_radians().tan()).abs() < 1e-3);
        let tiny = w.render_scene(day_concrete(), FramingF::FitWork, 0, 99_999);
        assert_eq!((tiny.width, tiny.height), (1, MAX_IMAGE_SIDE));
    }

    #[test]
    fn farther_viewing_shrinks_the_painted_area() {
        let w = work();
        let bare = SavedWork::new(serialize::encode(&Canvas::new(), 0)).unwrap();
        let painted_px = |meters: f32| {
            let f = FramingF::ViewingDistance { meters };
            let a = w.render_scene(day_concrete(), f, 200, 150).rgba;
            // Same view geometry for the empty wall: compare pixel by pixel.
            let mut layer = PaintLayer::new(w.view(f.into(), 200, 150));
            bare.stamp(&mut layer, 0..0);
            let b = scene::compose(&layer, day_concrete().into(), w.bounds);
            a.chunks(4).zip(b.chunks(4)).filter(|(p, q)| p != q).count()
        };
        let near = painted_px(1.5);
        let far = painted_px(6.0);
        assert!(near > 0 && far > 0);
        // 4x distance -> ~1/16 the area.
        let ratio = near as f32 / far as f32;
        assert!((10.0..24.0).contains(&ratio), "ratio {ratio}");
    }

    #[test]
    fn timelapse_ends_on_the_finished_work_and_starts_bare() {
        let w = work();
        let d = w.info().timelapse_duration_ms;
        let full = w.render_scene(day_concrete(), FramingF::FitWork, 96, 72);
        // Play forward in steps (incremental path), then compare.
        for t in (0..=d).step_by(37) {
            w.render_timelapse_frame(t, day_concrete(), 96, 72);
        }
        let last = w.render_timelapse_frame(d, day_concrete(), 96, 72);
        assert_eq!(last.rgba, full.rgba);
        assert_eq!(w.timelapse_progress(d).fraction, 1.0);

        // Scrubbing back re-renders from scratch, matching a fresh render.
        let mid = w.render_timelapse_frame(d / 3, day_concrete(), 96, 72);
        let fresh = work().render_timelapse_frame(d / 3, day_concrete(), 96, 72);
        assert_eq!(mid.rgba, fresh.rgba);
        assert_ne!(mid.rgba, full.rgba);

        let bare_wall = SavedWork::new(serialize::encode(&Canvas::new(), 0)).unwrap();
        let mut layer = PaintLayer::new(w.view(Framing::FitWork, 96, 72));
        bare_wall.stamp(&mut layer, 0..0);
        let first = w.timelapse_progress(0);
        assert_eq!(first.visible_dabs, 1);
        assert_eq!(first.active_stroke, Some(0));
    }

    #[test]
    fn timelapse_progress_walks_strokes_in_order() {
        let w = work();
        let p = w.timelapse_progress(230);
        assert_eq!(p.completed_strokes, 1);
        assert_eq!(p.active_stroke, None);
        assert_eq!((p.visible_dabs, p.visible_drips), (12, 1));
        let p = w.timelapse_progress(220 + crate::timelapse::MAX_PAUSE_MS + 10);
        assert_eq!(p.active_stroke, Some(1));
    }

    #[test]
    fn v2_work_replays_with_synthesized_timing() {
        let mut c = Canvas::new();
        c.commit_stroke(Stroke {
            palette_index: 1,
            dabs: vec![Dab::circle(Vec2::new(0.0, 0.0), 0.05, 1.0, 0.5); 5],
            drips: vec![],
            timing: None,
        });
        let w = SavedWork::new(serialize::encode(&c, 0)).unwrap();
        let i = w.info();
        assert!(!i.has_recorded_timing);
        assert_eq!(i.timelapse_duration_ms, 4 * crate::timelapse::SYNTH_DAB_MS);
    }
}
