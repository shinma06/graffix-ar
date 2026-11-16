//! Timelapse video export: which replay instant every video frame shows, the
//! video size, and the encoder bitrate. The shell only encodes the frames the
//! core renders (`TimelapseExport::render_frame`) and hands the file to the
//! share sheet.
//!
//! The replay is sped up when needed so the whole video, including a short
//! hold on the finished work, stays within `MAX_VIDEO_MS` (SNS-friendly).

use std::sync::{Arc, Mutex};

use crate::raster::View;
use crate::scene::Scene;
use crate::work::{RasterImageF, ReplayCache, SavedWork};

pub const MIN_FPS: u32 = 15;
pub const MAX_FPS: u32 = 60;
pub const DEFAULT_FPS: u32 = 30;
/// Slowest replay speed (×1 = as painted, after pause compression).
pub const MIN_SPEED: f32 = 1.0;
/// Fastest replay speed a caller may request; long works may still be sped
/// up beyond it to respect `MAX_VIDEO_MS`.
pub const MAX_REQUESTED_SPEED: f32 = 64.0;
/// Longest video, hold included, ms.
pub const MAX_VIDEO_MS: u32 = 60_000;
/// How long the finished work stays on screen at the end, ms.
pub const HOLD_MS: u32 = 1_500;
/// Video side limits, pixels (H.264 wants even sizes).
pub const MIN_SIDE: u32 = 64;
pub const MAX_SIDE: u32 = 1920;
/// Encoder bits per pixel per frame; the procedural wall texture is noisy.
const BITS_PER_PIXEL: f64 = 0.15;
pub const MIN_BITRATE_BPS: u32 = 1_000_000;
pub const MAX_BITRATE_BPS: u32 = 16_000_000;

/// What the caller asks for; every field is clamped to a supported value.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct TimelapseExportRequestF {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Replay speed multiplier (the player's ×1 / ×2 / ×4 / ×8).
    pub speed: f32,
}

/// The video that will be written.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct TimelapseExportPlanF {
    /// Even, aspect-preserving, within `MIN_SIDE..=MAX_SIDE`.
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Frames that show the replay; the last one is the finished work.
    pub replay_frame_count: u32,
    /// Extra frames repeating the finished work.
    pub hold_frame_count: u32,
    pub total_frame_count: u32,
    /// `total_frame_count` frames of `1 / fps` s each, ms.
    pub video_duration_ms: u32,
    /// Timelapse (replay) length being covered, ms.
    pub replay_duration_ms: u32,
    /// Speed actually used.
    pub speed: f32,
    /// Speed asked for, after clamping.
    pub requested_speed: f32,
    /// `speed` > `requested_speed` to stay within `max_video_ms`.
    pub sped_up: bool,
    pub max_video_ms: u32,
    pub average_bitrate_bps: u32,
}

/// Default request values for the export UI.
#[uniffi::export]
pub fn default_timelapse_export_request(speed: f32) -> TimelapseExportRequestF {
    TimelapseExportRequestF {
        width: 1080,
        height: 810,
        fps: DEFAULT_FPS,
        speed,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub speed: f32,
    pub requested_speed: f32,
    /// Replay time shown by each replay frame: starts at 0, strictly
    /// increasing, ends at the replay duration.
    pub frame_times_ms: Vec<u32>,
    pub hold_frames: u32,
    pub replay_duration_ms: u32,
    pub bitrate_bps: u32,
}

fn even_down(x: f64) -> u32 {
    ((x.floor() as u32) & !1).max(MIN_SIDE)
}

/// Fit the requested size into the supported range keeping its aspect.
pub fn video_size(width: u32, height: u32) -> (u32, u32) {
    let (w, h) = (width.max(1) as f64, height.max(1) as f64);
    let long = w.max(h);
    let short = w.min(h);
    // Scale down to MAX_SIDE, or up so the short side reaches MIN_SIDE;
    // extreme aspects end up clamped on one side.
    let scale = if long > MAX_SIDE as f64 {
        MAX_SIDE as f64 / long
    } else if short < MIN_SIDE as f64 {
        (MIN_SIDE as f64 / short).min(MAX_SIDE as f64 / long)
    } else {
        1.0
    };
    (
        even_down(w * scale).min(MAX_SIDE),
        even_down(h * scale).min(MAX_SIDE),
    )
}

fn frames_for_ms(ms: u32, fps: u32) -> u32 {
    (ms as u64 * fps as u64).div_ceil(1000) as u32
}

impl Plan {
    pub fn new(replay_duration_ms: u32, request: &TimelapseExportRequestF) -> Self {
        let fps = request.fps.clamp(MIN_FPS, MAX_FPS);
        let requested_speed = if request.speed.is_finite() {
            request.speed.clamp(MIN_SPEED, MAX_REQUESTED_SPEED)
        } else {
            MIN_SPEED
        };
        let hold_frames = frames_for_ms(HOLD_MS, fps);
        let max_frames = (MAX_VIDEO_MS as u64 * fps as u64 / 1000) as u32;
        // Replay steps after the first (t = 0) frame that still fit.
        let step_budget = max_frames - hold_frames - 1;

        let d = replay_duration_ms as f64;
        let fps_f = fps as f64;
        let needed = d * fps_f / (1000.0 * step_budget as f64);
        let speed = (requested_speed as f64).max(needed);
        let step_ms = 1000.0 * speed / fps_f;
        let steps = ((d / step_ms).ceil() as u32).min(step_budget);
        let mut frame_times_ms: Vec<u32> = (0..=steps)
            .map(|k| ((k as f64 * step_ms).floor() as u32).min(replay_duration_ms))
            .collect();
        *frame_times_ms.last_mut().unwrap() = replay_duration_ms;

        let (width, height) = video_size(request.width, request.height);
        let bitrate = (width as f64 * height as f64 * fps_f * BITS_PER_PIXEL)
            .clamp(MIN_BITRATE_BPS as f64, MAX_BITRATE_BPS as f64);
        Self {
            width,
            height,
            fps,
            speed: speed as f32,
            requested_speed,
            frame_times_ms,
            hold_frames,
            replay_duration_ms,
            bitrate_bps: bitrate as u32,
        }
    }

    pub fn replay_frames(&self) -> u32 {
        self.frame_times_ms.len() as u32
    }

    pub fn total_frames(&self) -> u32 {
        self.replay_frames() + self.hold_frames
    }

    pub fn video_duration_ms(&self) -> u32 {
        (self.total_frames() as u64 * 1000 / self.fps as u64) as u32
    }

    /// Replay time of video frame `index`; hold frames (and anything past
    /// the end) show the finished work.
    pub fn time_of(&self, index: u32) -> u32 {
        self.frame_times_ms
            .get(index as usize)
            .copied()
            .unwrap_or(self.replay_duration_ms)
    }

    pub fn to_ffi(&self) -> TimelapseExportPlanF {
        TimelapseExportPlanF {
            width: self.width,
            height: self.height,
            fps: self.fps,
            replay_frame_count: self.replay_frames(),
            hold_frame_count: self.hold_frames,
            total_frame_count: self.total_frames(),
            video_duration_ms: self.video_duration_ms(),
            replay_duration_ms: self.replay_duration_ms,
            speed: self.speed,
            requested_speed: self.requested_speed,
            sped_up: self.speed > self.requested_speed,
            max_video_ms: MAX_VIDEO_MS,
            average_bitrate_bps: self.bitrate_bps,
        }
    }
}

/// One export job: renders the video frames of a saved work in order. Has
/// its own replay cache, so it does not disturb the on-screen player.
#[derive(uniffi::Object)]
pub struct TimelapseExport {
    work: Arc<SavedWork>,
    plan: Plan,
    view: View,
    scene: Scene,
    cache: Mutex<Option<ReplayCache>>,
}

impl TimelapseExport {
    pub(crate) fn new(work: Arc<SavedWork>, plan: Plan, scene: Scene) -> Self {
        let view = work.fit_view(plan.width, plan.height);
        Self {
            work,
            plan,
            view,
            scene,
            cache: Mutex::new(None),
        }
    }
}

#[uniffi::export]
impl TimelapseExport {
    pub fn plan(&self) -> TimelapseExportPlanF {
        self.plan.to_ffi()
    }

    /// Video frame `index` (0-based) at the plan's size. Rendering in order
    /// is incremental; hold frames (index >= replay frame count) are the
    /// finished work.
    pub fn render_frame(&self, index: u32) -> RasterImageF {
        let mut cache = self.cache.lock().unwrap();
        self.work
            .render_replay(&mut cache, self.view, self.plan.time_of(index), self.scene)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{Canvas, Dab, Stroke, StrokeTiming};
    use crate::math::Vec2;
    use crate::serialize;
    use crate::work::{FramingF, SceneConfigF, SceneLightingF, WallMaterialF};

    fn req(fps: u32, speed: f32) -> TimelapseExportRequestF {
        TimelapseExportRequestF {
            width: 1080,
            height: 810,
            fps,
            speed,
        }
    }

    fn assert_well_formed(p: &Plan) {
        let t = &p.frame_times_ms;
        assert_eq!(t[0], 0);
        assert_eq!(*t.last().unwrap(), p.replay_duration_ms);
        assert!(t.windows(2).all(|w| w[0] < w[1]), "not increasing");
        assert!(p.video_duration_ms() <= MAX_VIDEO_MS);
        assert_eq!(p.total_frames(), p.replay_frames() + p.hold_frames);
    }

    #[test]
    fn short_work_plays_at_the_requested_speed() {
        // 10 s replay at ×2, 30 fps: 5 s of video, a frame every 66.7 ms.
        let p = Plan::new(10_000, &req(30, 2.0));
        assert_well_formed(&p);
        assert_eq!(p.speed, 2.0);
        assert!(!p.to_ffi().sped_up);
        assert_eq!(p.replay_frames(), 150 + 1);
        assert_eq!(p.frame_times_ms[1], 66);
        assert_eq!(p.frame_times_ms[3], 200);
        assert_eq!(p.hold_frames, 45);
        let f = p.to_ffi();
        assert_eq!(f.total_frame_count, 196);
        assert_eq!(f.video_duration_ms, 196 * 1000 / 30);
    }

    #[test]
    fn frame_spacing_matches_speed_over_fps() {
        for (fps, speed) in [(24, 1.0), (30, 4.0), (60, 8.0), (60, 1.0)] {
            let p = Plan::new(7_777, &req(fps, speed));
            assert_well_formed(&p);
            let step = 1000.0 * speed / fps as f32;
            for w in p.frame_times_ms[..p.frame_times_ms.len() - 1].windows(2) {
                let d = (w[1] - w[0]) as f32;
                assert!((d - step).abs() <= 1.0, "fps {fps} speed {speed}: {d}");
            }
            // The final step lands exactly on the end, never past it.
            let n = p.frame_times_ms.len();
            assert!((p.frame_times_ms[n - 1] - p.frame_times_ms[n - 2]) as f32 <= step + 1.0);
        }
    }

    #[test]
    fn long_work_is_sped_up_to_fit_the_limit() {
        // A one-hour replay at ×2 would be 30 minutes.
        let p = Plan::new(3_600_000, &req(30, 2.0));
        assert_well_formed(&p);
        let f = p.to_ffi();
        assert!(f.sped_up);
        assert_eq!(f.requested_speed, 2.0);
        assert!(f.speed > 60.0, "speed {}", f.speed);
        assert_eq!(f.total_frame_count, MAX_VIDEO_MS * 30 / 1000);
        assert!(f.video_duration_ms <= MAX_VIDEO_MS);
        // The hold is never eaten by the speed-up.
        assert_eq!(f.hold_frame_count, 45);
    }

    #[test]
    fn duration_limit_holds_for_any_length_and_rate() {
        for d in [
            0,
            1,
            999,
            58_499,
            58_500,
            58_501,
            117_000,
            1_000_000,
            u32::MAX,
        ] {
            for fps in [MIN_FPS, 24, 25, 30, 59, MAX_FPS] {
                for speed in [1.0, 3.0, 8.0] {
                    let p = Plan::new(d, &req(fps, speed));
                    assert_well_formed(&p);
                    assert!(p.speed >= p.requested_speed);
                }
            }
        }
    }

    #[test]
    fn instant_work_is_one_frame_then_the_hold() {
        let p = Plan::new(0, &req(30, 1.0));
        assert_eq!(p.frame_times_ms, vec![0]);
        assert_eq!(p.time_of(0), 0);
        assert_eq!(p.time_of(40), 0);
        assert_eq!(p.to_ffi().video_duration_ms, 46 * 1000 / 30);
    }

    #[test]
    fn hold_frames_show_the_finished_work() {
        let p = Plan::new(5_000, &req(30, 4.0));
        let last = p.replay_frames() - 1;
        assert_eq!(p.time_of(last), 5_000);
        for i in last..p.total_frames() + 3 {
            assert_eq!(p.time_of(i), 5_000);
        }
        assert!(p.time_of(last - 1) < 5_000);
    }

    #[test]
    fn request_values_are_clamped() {
        let p = Plan::new(1_000, &req(1, -3.0));
        assert_eq!((p.fps, p.speed, p.requested_speed), (MIN_FPS, 1.0, 1.0));
        let p = Plan::new(1_000, &req(240, f32::NAN));
        assert_eq!((p.fps, p.requested_speed), (MAX_FPS, MIN_SPEED));
        let p = Plan::new(1_000, &req(30, 1_000.0));
        assert_eq!(p.requested_speed, MAX_REQUESTED_SPEED);
        let d = default_timelapse_export_request(4.0);
        assert_eq!((d.fps, d.speed), (DEFAULT_FPS, 4.0));
        assert_eq!(video_size(d.width, d.height), (d.width, d.height));
    }

    #[test]
    fn video_size_is_even_bounded_and_keeps_aspect() {
        assert_eq!(video_size(1080, 810), (1080, 810));
        assert_eq!(video_size(801, 601), (800, 600));
        assert_eq!(video_size(3840, 2880), (1920, 1440));
        assert_eq!(video_size(2160, 3840), (1080, 1920));
        assert_eq!(video_size(0, 0), (MIN_SIDE, MIN_SIDE));
        assert_eq!(video_size(32, 24), (84, 64));
        // Extreme aspect: long side capped, short side floored.
        assert_eq!(video_size(10_000, 10), (MAX_SIDE, MIN_SIDE));
        for (w, h) in [(1, 9_999), (333, 777), (1919, 1081), (64, 63)] {
            let (a, b) = video_size(w, h);
            assert!(a % 2 == 0 && b % 2 == 0);
            assert!((MIN_SIDE..=MAX_SIDE).contains(&a) && (MIN_SIDE..=MAX_SIDE).contains(&b));
        }
    }

    #[test]
    fn bitrate_grows_with_pixels_and_stays_bounded() {
        let small = Plan::new(1_000, &req(30, 1.0));
        let mut big_req = req(60, 1.0);
        (big_req.width, big_req.height) = (1920, 1440);
        let big = Plan::new(1_000, &big_req);
        assert!(big.bitrate_bps > small.bitrate_bps);
        assert!(big.bitrate_bps <= MAX_BITRATE_BPS);
        let mut tiny = req(15, 1.0);
        (tiny.width, tiny.height) = (64, 64);
        assert_eq!(Plan::new(1_000, &tiny).bitrate_bps, MIN_BITRATE_BPS);
    }

    fn work() -> Arc<SavedWork> {
        let mut c = Canvas::new();
        for k in 0..3u32 {
            c.commit_stroke(Stroke {
                palette_index: k as u8 + 2,
                dabs: (0..10)
                    .map(|i| {
                        Dab::circle(
                            Vec2::new(-0.3 + i as f32 * 0.06, 0.15 * k as f32),
                            0.05,
                            0.9,
                            0.4,
                        )
                    })
                    .collect(),
                drips: vec![],
                timing: Some(StrokeTiming {
                    start_ms: 1_000 * k,
                    dab_ms: (0..10).map(|i| i * 30).collect(),
                    drip_ms: vec![],
                }),
            });
        }
        SavedWork::new(serialize::encode(&c, 0)).unwrap()
    }

    fn scene() -> SceneConfigF {
        SceneConfigF {
            lighting: SceneLightingF::Overcast,
            material: WallMaterialF::Plaster,
            wall_color: None,
        }
    }

    fn small(speed: f32) -> TimelapseExportRequestF {
        TimelapseExportRequestF {
            width: 97,
            height: 73,
            fps: 30,
            speed,
        }
    }

    #[test]
    fn export_frames_replay_the_work_and_hold_the_result() {
        let w = work();
        let job = w.clone().timelapse_export(small(1.0), scene());
        let plan = job.plan();
        assert_eq!(plan, w.timelapse_export_plan(small(1.0)));
        assert_eq!(plan.replay_duration_ms, w.info().timelapse_duration_ms);
        assert_eq!((plan.width, plan.height), (96, 72));

        let full = w.render_scene(scene(), FramingF::FitWork, 96, 72);
        let frames: Vec<_> = (0..plan.total_frame_count)
            .map(|i| job.render_frame(i))
            .collect();
        assert!(frames.iter().all(|f| (f.width, f.height) == (96, 72)));
        assert_ne!(frames[0].rgba, full.rgba);
        let last_replay = (plan.replay_frame_count - 1) as usize;
        assert_ne!(frames[last_replay - 1].rgba, full.rgba);
        for f in &frames[last_replay..] {
            assert_eq!(f.rgba, full.rgba);
        }
        // Frames match the player's frames at the same replay times.
        for i in [0, last_replay / 2, last_replay] {
            let t = Plan::new(plan.replay_duration_ms, &small(1.0)).time_of(i as u32);
            let player = work()
                .replay()
                .render_frame(t, scene(), FramingF::FitWork, 96, 72);
            assert_eq!(frames[i].rgba, player.rgba, "frame {i}");
        }
    }

    #[test]
    fn export_does_not_disturb_the_player() {
        let w = work();
        let job = w.clone().timelapse_export(small(2.0), scene());
        let n = job.plan().replay_frame_count;
        let replay = w.clone().replay();
        let mut player = Vec::new();
        for i in 0..n {
            job.render_frame(i);
            player.push(
                replay
                    .render_frame(i * 20, scene(), FramingF::FitWork, 80, 60)
                    .rgba,
            );
        }
        let fresh = work().replay();
        for (i, rgba) in player.iter().enumerate() {
            let want = fresh.render_frame(i as u32 * 20, scene(), FramingF::FitWork, 80, 60);
            assert_eq!(*rgba, want.rgba);
        }
        // And re-rendering an earlier export frame (e.g. a retry) is exact.
        let again = job.render_frame(1);
        let fresh_job = work().timelapse_export(small(2.0), scene());
        assert_eq!(again.rgba, fresh_job.render_frame(1).rgba);
    }
}
