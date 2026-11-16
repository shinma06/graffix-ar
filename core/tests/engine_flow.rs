//! End-to-end exercise of the UniFFI-facing Engine: planes in, wall lock,
//! spray strokes, modes/caps/shake, undo, and canvas serialization
//! round-trip — the whole core loop the shell drives, minus the rendering.

use graffix_core::engine::{
    supported_brush_presets, supported_spray_caps, supported_spray_colors, Engine,
};
use graffix_core::ffi::*;
use std::sync::Arc;

fn v3(x: f32, y: f32, z: f32) -> Vec3F {
    Vec3F { x, y, z }
}

fn wall(id: u64, center: Vec3F) -> PlaneObservationF {
    PlaneObservationF {
        id,
        center,
        normal: v3(0.0, 0.0, -1.0),
        axis_u: v3(1.0, 0.0, 0.0),
        half_extent_u: 1.0,
        half_extent_v: 1.0,
    }
}

fn input(ray_dir: Vec3F, spraying: bool) -> FrameInputF {
    input_dt(ray_dir, spraying, 1.0 / 60.0)
}

fn input_dt(ray_dir: Vec3F, spraying: bool, dt: f32) -> FrameInputF {
    FrameInputF {
        camera_position: v3(0.0, 1.5, 0.0),
        ray_origin: v3(0.0, 1.5, 0.0),
        ray_dir,
        plane_upserts: vec![],
        plane_removals: vec![],
        lidar_center_depth_m: None,
        tracking: TrackingStateF::Normal,
        spraying,
        dt_seconds: dt,
    }
}

fn forward() -> Vec3F {
    v3(0.0, 0.0, 1.0)
}

/// Spawn an engine with one wall `z` meters ahead, aimed and locked.
fn locked_engine(wall_z: f32) -> Arc<Engine> {
    let e = Engine::new();
    let mut f = input(forward(), false);
    f.plane_upserts.push(wall(42, v3(0.0, 1.5, wall_z)));
    e.tick(f);
    assert_eq!(e.lock_aimed(), Some(42));
    e
}

#[test]
fn aim_lock_spray_serialize() {
    let e = Engine::new();

    // Wall appears 2 m ahead.
    let mut f = input(forward(), false);
    f.plane_upserts.push(wall(42, v3(0.0, 1.5, 2.0)));
    let out = e.tick(f);
    assert_eq!(out.aimed_wall_id, Some(42));
    assert_eq!(e.wall_count(), 1);

    // Lock the aimed wall.
    assert_eq!(e.lock_aimed(), Some(42));

    // Hold spray and sweep the nozzle sideways across 5 frames.
    let mut dab_total = 0;
    for i in 1..=5 {
        let out = e.tick(input(v3(i as f32 * 0.1, 0.0, 2.0), true));
        assert!(out.stroke_active);
        assert!(out.spray.spraying);
        dab_total += out.dabs_added;
    }
    assert!(dab_total > 0);

    // Release: stroke commits.
    let out = e.tick(input(forward(), false));
    assert!(!out.stroke_active);
    assert!(!out.spray.spraying);
    let snap = e.canvas_snapshot();
    assert_eq!(snap.strokes.len(), 1);
    assert_eq!(snap.dab_count, dab_total);
    assert!(snap.frame.is_some());

    // Undo removes the stroke; redo brings it back.
    assert!(e.undo());
    assert_eq!(e.canvas_snapshot().dab_count, 0);
    assert!(e.redo());
    assert_eq!(e.canvas_snapshot().dab_count, dab_total);

    // Serialize -> load into a fresh engine -> identical canvas.
    let bytes = e.serialize_canvas();
    let e2 = Engine::new();
    e2.load_canvas(bytes).unwrap();
    let snap2 = e2.canvas_snapshot();
    assert_eq!(snap2.dab_count, dab_total);
    assert_eq!(snap2.strokes.len(), 1);
}

#[test]
fn no_spray_without_lock_and_corrupt_load_fails() {
    let e = Engine::new();
    let mut f = input(forward(), true);
    f.plane_upserts.push(wall(1, v3(0.0, 1.5, 2.0)));
    // Spraying while only aimed (not locked) paints nothing.
    let out = e.tick(f);
    assert_eq!(out.dabs_added, 0);
    assert!(!out.stroke_active);
    assert_eq!(e.canvas_snapshot().dab_count, 0);

    // Corrupt bytes are rejected.
    assert!(e.load_canvas(b"not a canvas".to_vec()).is_err());
}

#[test]
fn unlock_stops_stroke_and_lost_wall_unlocks() {
    let e = Engine::new();
    let mut f = input(forward(), false);
    f.plane_upserts.push(wall(9, v3(0.0, 1.5, 2.0)));
    e.tick(f);
    e.lock_aimed();
    e.tick(input(v3(0.2, 0.0, 2.0), true));
    assert!(e.canvas_snapshot().dab_count > 0);

    // Plane disappears: engine drops the lock and commits the stroke.
    let mut f = input(forward(), true);
    f.plane_removals.push(9);
    let out = e.tick(f);
    assert_eq!(out.locked_wall_id, None);
    assert!(!out.stroke_active);
}

#[test]
fn real_mode_tilt_produces_elliptical_dabs() {
    let e = locked_engine(1.0);
    e.set_mode(SprayModeF::Real);
    // Fresh can for clean output.
    for _ in 0..3 {
        e.shake();
    }

    // Head-on stamp: a circle (major == minor, no gradient).
    e.tick(input(forward(), true));
    e.tick(input(forward(), false));
    // Tilted stroke: ray leans +x in world space.
    e.tick(input(v3(0.5, 0.0, 0.866), true));
    e.tick(input(forward(), false));

    let snap = e.canvas_snapshot();
    assert_eq!(snap.strokes.len(), 2);
    let head_on = snap.strokes[0].dabs[0];
    assert!((head_on.radius_major_m - head_on.radius_minor_m).abs() < 1e-6);
    let tilted = snap.strokes[1].dabs[0];
    assert!(
        tilted.radius_major_m > tilted.radius_minor_m * 1.05,
        "tilted dab should be elliptical: {:?}",
        tilted
    );
    // Canvas u points world -x; the ray leans +x so the dense (near) side is
    // world -x = +u in wall-local space.
    assert!(tilted.gradient_u > 0.5, "gradient {:?}", tilted.gradient_u);
}

#[test]
fn caps_change_the_footprint() {
    let e = locked_engine(1.0);
    e.set_mode(SprayModeF::Real);
    for _ in 0..3 {
        e.shake();
    }
    assert_eq!(
        supported_spray_caps(),
        [
            SprayCapF::Fat,
            SprayCapF::Standard,
            SprayCapF::Skinny,
            SprayCapF::Needle
        ]
    );
    assert_eq!(supported_brush_presets().len(), 4);
    assert_eq!(e.cap(), SprayCapF::Standard);

    e.set_cap(SprayCapF::Needle);
    e.tick(input(forward(), true));
    e.tick(input(forward(), false));
    e.set_cap(SprayCapF::Fat);
    e.tick(input(v3(0.3, 0.0, 1.0), true));
    e.tick(input(forward(), false));

    let snap = e.canvas_snapshot();
    let needle = snap.strokes[0].dabs[0];
    let fat = snap.strokes[1].dabs[0];
    assert!(
        fat.radius_minor_m > needle.radius_minor_m * 3.0,
        "fat cap should dwarf needle: needle={} fat={}",
        needle.radius_minor_m,
        fat.radius_minor_m
    );
}

#[test]
fn shake_level_decays_and_low_shake_speckles_real_mode() {
    // Decay: three shakes fill the can; ~8 s of 0.5 s ticks drop it to ~60%.
    let e = Engine::new();
    for _ in 0..3 {
        e.shake();
    }
    assert!(e.shake_level() > 0.95);
    for _ in 0..8 {
        e.tick(input_dt(forward(), false, 0.5));
    }
    let level = e.shake_level();
    assert!(level < 0.75 && level > 0.4, "level after decay: {level}");
    // Decay also flows through the per-frame spray state.
    let out = e.tick(input(forward(), false));
    assert!((out.spray.shake_level - e.shake_level()).abs() < 1e-6);

    // Speckle: unshaken dabs are fainter than shaken dabs in Real mode.
    let e2 = locked_engine(0.5);
    e2.set_mode(SprayModeF::Real);
    e2.tick(input(forward(), true));
    e2.tick(input(forward(), false));
    let faint = e2.canvas_snapshot().strokes[0].dabs[0].opacity;
    for _ in 0..3 {
        e2.shake();
    }
    e2.tick(input(v3(0.05, 0.0, 0.5), true));
    e2.tick(input(forward(), false));
    let snap = e2.canvas_snapshot();
    let strong = snap.strokes[1].dabs[0].opacity;
    assert!(faint < strong * 0.7, "faint={faint} strong={strong}");

    // Casual mode never speckles: same spot, no shakes, preset opacity.
    let e3 = locked_engine(0.5);
    e3.tick(input(forward(), true));
    e3.tick(input(forward(), false));
    let casual = e3.canvas_snapshot().strokes[0].dabs[0].opacity;
    assert!((casual - 0.90).abs() < 1e-6); // 太ブラシ preset
}

#[test]
fn stationary_spray_drips_in_real_mode_only() {
    // Close wall (inside the opacity plateau), freshly shaken can, held
    // still: paint pools and must eventually run.
    let e = locked_engine(0.3);
    e.set_mode(SprayModeF::Real);
    for _ in 0..3 {
        e.shake();
    }
    for _ in 0..80 {
        e.tick(input(forward(), true));
    }
    let snap = e.canvas_snapshot();
    let drips: usize = snap.strokes.iter().map(|s| s.drips.len()).sum();
    assert!(drips >= 1, "oversaturated wall should drip");
    let drip = snap.strokes[0].drips[0];
    assert!(drip.len_m > 0.0 && drip.width_m > 0.0);

    // Same dwelling in casual mode never saturates: no drips, stable preset.
    let e2 = locked_engine(0.3);
    for _ in 0..80 {
        e2.tick(input(forward(), true));
    }
    let snap2 = e2.canvas_snapshot();
    let drips2: usize = snap2.strokes.iter().map(|s| s.drips.len()).sum();
    assert_eq!(drips2, 0);
}

#[test]
fn saved_work_replays_recorded_stroke_timing() {
    use graffix_core::work::SavedWork;

    let e = locked_engine(2.0);
    // Stroke 1: 10 frames of 50 ms sweeping right.
    for i in 1..=10 {
        e.tick(input_dt(v3(i as f32 * 0.03, 0.0, 2.0), true, 0.05));
    }
    // 3 s idle (60 frames of 50 ms), then stroke 2: 5 frames.
    for _ in 0..60 {
        e.tick(input_dt(forward(), false, 0.05));
    }
    for i in 1..=5 {
        e.tick(input_dt(v3(0.0, i as f32 * 0.03, 2.0), true, 0.05));
    }
    e.tick(input_dt(forward(), false, 0.05));

    let work = SavedWork::new(e.serialize_canvas()).unwrap();
    let info = work.info();
    assert_eq!(info.stroke_count, 2);
    assert!(info.has_recorded_timing);
    // Painting took ~450 ms + ~200 ms; the 3 s idle is compressed.
    assert!(
        (500..=1_200).contains(&info.timelapse_duration_ms),
        "duration {}",
        info.timelapse_duration_ms
    );
    let half = work.timelapse_progress(200);
    assert_eq!(half.active_stroke, Some(0));
    assert!(half.visible_dabs > 0 && half.visible_dabs < info.dab_count);
    let done = work.timelapse_progress(info.timelapse_duration_ms);
    assert_eq!(done.completed_strokes, 2);
    assert_eq!(done.visible_dabs, info.dab_count);

    // Loading the work and painting more keeps the timeline ordered.
    let e2 = locked_engine(2.0);
    e2.load_canvas(e.serialize_canvas()).unwrap();
    e2.lock_aimed();
    for i in 1..=3 {
        e2.tick(input_dt(v3(-(i as f32) * 0.03, 0.0, 2.0), true, 0.05));
    }
    e2.tick(input_dt(forward(), false, 0.05));
    let more = SavedWork::new(e2.serialize_canvas()).unwrap().info();
    assert_eq!(more.stroke_count, 3);
    assert!(more.has_recorded_timing);
    assert!(more.timelapse_duration_ms > info.timelapse_duration_ms);
}

#[test]
fn saved_work_exports_timelapse_video_frames() {
    use graffix_core::export::TimelapseExportRequestF;
    use graffix_core::work::{FramingF, SavedWork, SceneConfigF, SceneLightingF, WallMaterialF};

    let e = locked_engine(2.0);
    for i in 1..=20 {
        e.tick(input_dt(v3(i as f32 * 0.02, 0.0, 2.0), true, 0.05));
    }
    e.tick(input_dt(forward(), false, 0.05));

    let work = SavedWork::new(e.serialize_canvas()).unwrap();
    let request = TimelapseExportRequestF {
        width: 120,
        height: 90,
        fps: 30,
        speed: 2.0,
    };
    let scene = SceneConfigF {
        lighting: SceneLightingF::Overcast,
        material: WallMaterialF::Plaster,
        wall_color: None,
    };
    let job = work.clone().timelapse_export(request, scene);
    let plan = job.plan();
    assert_eq!(plan, work.timelapse_export_plan(request));
    assert!(!plan.sped_up);
    assert_eq!(plan.replay_duration_ms, work.info().timelapse_duration_ms);
    assert!(plan.replay_frame_count > 2);
    assert_eq!(
        plan.total_frame_count,
        plan.replay_frame_count + plan.hold_frame_count
    );

    let first = job.render_frame(0);
    assert_eq!((first.width, first.height), (120, 90));
    for i in 1..plan.total_frame_count - 1 {
        job.render_frame(i);
    }
    let last = job.render_frame(plan.total_frame_count - 1);
    let finished = work.render_scene(scene, FramingF::FitWork, 120, 90);
    assert_eq!(last.rgba, finished.rgba);
    assert_ne!(first.rgba, finished.rgba);
}

#[test]
fn spray_color_id_maps_to_the_stored_palette_index() {
    let e = locked_engine(1.0);
    assert_eq!(e.color(), SprayColorF::White);
    assert_eq!(e.preset(), supported_brush_presets()[0]);
    e.set_color(SprayColorF::Red);
    assert_eq!(e.color(), SprayColorF::Red);
    e.tick(input(forward(), true));
    e.tick(input(forward(), false));
    let snap = e.canvas_snapshot();
    let colors = supported_spray_colors();
    let red = colors
        .iter()
        .position(|c| c.id == SprayColorF::Red)
        .unwrap();
    assert_eq!(snap.strokes[0].palette_index as usize, red);
    assert_eq!(red, 3);
    assert_eq!(snap.strokes[0].color, colors[red].rgba);
}

#[test]
fn live_canvas_texture_tracks_spray_and_edits() {
    let e = locked_engine(2.0);
    assert!(e.render_live_canvas().is_none(), "no paint yet");

    // Painting shows up while the stroke is still in flight.
    let mut images = Vec::new();
    for i in 1..=6 {
        let out = e.tick(input(v3(i as f32 * 0.05, 0.0, 2.0), true));
        assert!(out.canvas_dirty);
        let img = e.render_live_canvas().expect("in-flight paint is drawn");
        assert_eq!(img.rgba.len(), (img.width * img.height * 4) as usize);
        assert!((img.width as f32 / img.pixels_per_meter - img.width_m).abs() < 1e-3);
        assert!((img.height as f32 / img.pixels_per_meter - img.height_m).abs() < 1e-3);
        images.push(img);
    }
    // Rendering marks the canvas clean.
    let out = e.tick(input(forward(), false));
    assert!(out.canvas_dirty, "commit bumps the revision");
    let live = e.render_live_canvas().unwrap();
    assert!(!e.tick(input(forward(), false)).canvas_dirty);
    assert!(live.rgba.chunks(4).any(|p| p[3] > 0));
    assert_eq!(live.rgba[..4], [0, 0, 0, 0], "transparent margin");
    assert!(live
        .rgba
        .chunks(4)
        .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]));

    // Undo/redo/load re-render the same pixels as a fresh engine.
    assert!(e.undo());
    assert!(e.render_live_canvas().is_none());
    assert!(e.redo());
    let redone = e.render_live_canvas().unwrap();
    assert_eq!(redone.rgba, live.rgba);

    let e2 = Engine::new();
    e2.load_canvas(e.serialize_canvas()).unwrap();
    let loaded = e2.render_live_canvas().unwrap();
    assert_eq!(loaded.rgba, live.rgba);
    assert_eq!(
        (loaded.min_u, loaded.min_v, loaded.width, loaded.height),
        (live.min_u, live.min_v, live.width, live.height)
    );

    e.clear();
    assert!(e.render_live_canvas().is_none());
}
