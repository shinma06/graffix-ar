//! Drawing timelapse: the order and replay time of every mark in a work.
//!
//! v3 works carry recorded per-mark timing (`StrokeTiming`); pre-v3 works do
//! not, so their schedule is synthesized from stroke order at a steady dab
//! rate. Long idle pauses (between or inside strokes) are compressed so the
//! replay shows painting, not waiting.

use crate::canvas::Stroke;

/// Longest pause kept in the replay, ms.
pub const MAX_PAUSE_MS: u32 = 400;
/// Pause between strokes when timing is unknown, ms.
pub const SYNTH_GAP_MS: u32 = 250;
/// Dab cadence when timing is unknown (~60 Hz painting), ms.
pub const SYNTH_DAB_MS: u32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    Dab(u32),
    Drip(u32),
}

/// One mark (dab or drip) of `stroke`, revealed at replay time `t_ms`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    pub t_ms: u32,
    pub stroke: u32,
    pub kind: MarkKind,
}

/// Replay progress at some time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    /// Marks visible (a prefix of `Timeline::marks`).
    pub visible_marks: usize,
    pub visible_dabs: u32,
    pub visible_drips: u32,
    pub completed_strokes: u32,
    /// Stroke being painted at this time, if any.
    pub active_stroke: Option<u32>,
}

#[derive(Clone, Debug, Default)]
pub struct Timeline {
    /// Paint order: stroke order, and inside a stroke by time (a drip shows
    /// up after the dabs that saturated it). `t_ms` is non-decreasing.
    pub marks: Vec<Mark>,
    /// Replay (start, end) per stroke, non-decreasing.
    pub spans: Vec<(u32, u32)>,
    /// Dab count among `marks[..i]`, for i in 0..=marks.len().
    dab_prefix: Vec<u32>,
    pub duration_ms: u32,
    /// Every stroke carried recorded timing.
    pub recorded: bool,
}

impl Timeline {
    pub fn build(strokes: &[Stroke]) -> Self {
        let mut marks =
            Vec::with_capacity(strokes.iter().map(|s| s.dabs.len() + s.drips.len()).sum());
        let mut spans = Vec::with_capacity(strokes.len());
        let mut cursor = 0u32;
        let mut prev_recorded_end: Option<u32> = None;
        let mut recorded = true;
        for (i, stroke) in strokes.iter().enumerate() {
            let timing = stroke.timing.as_ref().filter(|t| {
                t.dab_ms.len() == stroke.dabs.len() && t.drip_ms.len() == stroke.drips.len()
            });
            recorded &= timing.is_some();
            let gap = match (i, timing, prev_recorded_end) {
                (0, _, _) => 0,
                (_, Some(t), Some(prev)) => t.start_ms.saturating_sub(prev).min(MAX_PAUSE_MS),
                _ => SYNTH_GAP_MS,
            };
            let start = cursor.saturating_add(gap);

            // (offset in stroke, kind), merged by time; dabs first on ties.
            let mut local: Vec<(u32, MarkKind)> =
                Vec::with_capacity(stroke.dabs.len() + stroke.drips.len());
            match timing {
                Some(t) => {
                    let (mut d, mut r) = (0usize, 0usize);
                    while d < t.dab_ms.len() || r < t.drip_ms.len() {
                        let take_dab = r >= t.drip_ms.len()
                            || (d < t.dab_ms.len() && t.dab_ms[d] <= t.drip_ms[r]);
                        if take_dab {
                            local.push((t.dab_ms[d], MarkKind::Dab(d as u32)));
                            d += 1;
                        } else {
                            local.push((t.drip_ms[r], MarkKind::Drip(r as u32)));
                            r += 1;
                        }
                    }
                }
                None => {
                    for d in 0..stroke.dabs.len() {
                        local.push((d as u32 * SYNTH_DAB_MS, MarkKind::Dab(d as u32)));
                    }
                    let end = stroke.dabs.len().saturating_sub(1) as u32 * SYNTH_DAB_MS;
                    for r in 0..stroke.drips.len() {
                        local.push((end, MarkKind::Drip(r as u32)));
                    }
                }
            }

            // Compress pauses inside the stroke too (e.g. nozzle off the wall
            // while the button stayed down).
            let mut t = start;
            let mut prev_off = local.first().map_or(0, |m| m.0);
            for &(off, kind) in &local {
                t = t.saturating_add(off.saturating_sub(prev_off).min(MAX_PAUSE_MS));
                prev_off = off;
                marks.push(Mark {
                    t_ms: t,
                    stroke: i as u32,
                    kind,
                });
            }
            spans.push((start, t));
            cursor = t;
            prev_recorded_end = timing.map(|t| t.end_ms());
        }
        let mut dab_prefix = Vec::with_capacity(marks.len() + 1);
        let mut n = 0u32;
        dab_prefix.push(0);
        for m in &marks {
            n += matches!(m.kind, MarkKind::Dab(_)) as u32;
            dab_prefix.push(n);
        }
        Self {
            marks,
            spans,
            dab_prefix,
            duration_ms: cursor,
            recorded: recorded && !strokes.is_empty(),
        }
    }

    /// What is on the wall at replay time `t_ms` (marks with time <= t).
    pub fn progress(&self, t_ms: u32) -> Progress {
        let visible = self.marks.partition_point(|m| m.t_ms <= t_ms);
        let visible_dabs = self.dab_prefix.get(visible).copied().unwrap_or(0);
        let completed = self.spans.partition_point(|&(_, end)| end <= t_ms) as u32;
        let active_stroke = self
            .spans
            .get(completed as usize)
            .filter(|&&(start, _)| start <= t_ms)
            .map(|_| completed);
        Progress {
            visible_marks: visible,
            visible_dabs,
            visible_drips: visible as u32 - visible_dabs,
            completed_strokes: completed,
            active_stroke,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{Dab, Drip, StrokeTiming};
    use crate::math::Vec2;

    fn stroke(dabs: usize, drips: usize, timing: Option<StrokeTiming>) -> Stroke {
        Stroke {
            palette_index: 0,
            dabs: (0..dabs)
                .map(|i| Dab::circle(Vec2::new(i as f32 * 0.01, 0.0), 0.02, 1.0, 0.5))
                .collect(),
            drips: (0..drips)
                .map(|_| Drip {
                    uv: Vec2::new(0.0, 0.0),
                    len: 0.1,
                    width: 0.005,
                    opacity: 1.0,
                })
                .collect(),
            timing,
        }
    }

    fn timed(start_ms: u32, dab_ms: Vec<u32>, drip_ms: Vec<u32>) -> Option<StrokeTiming> {
        Some(StrokeTiming {
            start_ms,
            dab_ms,
            drip_ms,
        })
    }

    #[test]
    fn untimed_strokes_get_a_synthesized_schedule() {
        let tl = Timeline::build(&[stroke(3, 1, None), stroke(2, 0, None)]);
        assert!(!tl.recorded);
        let times: Vec<u32> = tl.marks.iter().map(|m| m.t_ms).collect();
        // Stroke 0: dabs at 0, 16, 32; drip at stroke end. Stroke 1 after a gap.
        let s1 = 32 + SYNTH_GAP_MS;
        assert_eq!(times, vec![0, 16, 32, 32, s1, s1 + 16]);
        assert_eq!(tl.marks[3].kind, MarkKind::Drip(0));
        assert_eq!(tl.spans, vec![(0, 32), (s1, s1 + 16)]);
        assert_eq!(tl.duration_ms, s1 + 16);
    }

    #[test]
    fn recorded_timing_is_replayed_with_pauses_compressed() {
        let tl = Timeline::build(&[
            stroke(3, 1, timed(1_000, vec![0, 20, 40], vec![20])),
            // Starts 60 s after the first ended: compressed to MAX_PAUSE_MS.
            stroke(2, 0, timed(61_040, vec![0, 5_000], vec![])),
        ]);
        assert!(tl.recorded);
        let got: Vec<(u32, MarkKind)> = tl.marks.iter().map(|m| (m.t_ms, m.kind)).collect();
        let s1 = 40 + MAX_PAUSE_MS;
        assert_eq!(
            got,
            vec![
                (0, MarkKind::Dab(0)),
                (20, MarkKind::Dab(1)),
                (20, MarkKind::Drip(0)), // after the dab that saturated it
                (40, MarkKind::Dab(2)),
                (s1, MarkKind::Dab(0)),
                (s1 + MAX_PAUSE_MS, MarkKind::Dab(1)),
            ]
        );
        assert_eq!(tl.duration_ms, s1 + MAX_PAUSE_MS);
    }

    #[test]
    fn short_recorded_gaps_are_kept() {
        let tl = Timeline::build(&[
            stroke(1, 0, timed(0, vec![0], vec![])),
            stroke(1, 0, timed(150, vec![0], vec![])),
        ]);
        assert_eq!(tl.spans[1].0, 150);
    }

    #[test]
    fn mixed_v2_then_v3_strokes() {
        // A v2 work loaded, extended with new (timed) strokes, re-saved.
        let tl = Timeline::build(&[
            stroke(2, 0, None),
            stroke(2, 0, timed(5_000, vec![0, 30], vec![])),
        ]);
        assert!(!tl.recorded);
        assert_eq!(tl.spans[1].0, 16 + SYNTH_GAP_MS);
        assert_eq!(tl.spans[1].1, 16 + SYNTH_GAP_MS + 30);
    }

    #[test]
    fn mismatched_timing_falls_back_to_synthesized() {
        let tl = Timeline::build(&[stroke(3, 0, timed(0, vec![0], vec![]))]);
        assert!(!tl.recorded);
        assert_eq!(tl.marks.len(), 3);
        assert_eq!(tl.marks[2].t_ms, 2 * SYNTH_DAB_MS);
    }

    #[test]
    fn progress_is_monotonic_and_complete() {
        let tl = Timeline::build(&[
            stroke(4, 2, timed(0, vec![0, 10, 20, 30], vec![10, 30])),
            stroke(3, 0, None),
        ]);
        let start = tl.progress(0);
        assert_eq!(start.visible_dabs, 1);
        assert_eq!(start.active_stroke, Some(0));
        let mut prev = 0;
        for t in (0..=tl.duration_ms).step_by(5) {
            let p = tl.progress(t);
            assert!(p.visible_marks >= prev);
            assert_eq!(p.visible_marks as u32, p.visible_dabs + p.visible_drips);
            prev = p.visible_marks;
        }
        // Between strokes nothing is active.
        let gap = tl.progress(tl.spans[0].1 + 1);
        assert_eq!(gap.completed_strokes, 1);
        assert_eq!(gap.active_stroke, None);
        let end = tl.progress(tl.duration_ms);
        assert_eq!(end.visible_marks, tl.marks.len());
        assert_eq!((end.visible_dabs, end.visible_drips), (7, 2));
        assert_eq!(end.completed_strokes, 2);
        assert_eq!(tl.progress(u32::MAX), end);
    }

    #[test]
    fn empty_work_has_empty_timeline() {
        let tl = Timeline::build(&[]);
        assert_eq!(tl.duration_ms, 0);
        assert!(!tl.recorded);
        let p = tl.progress(0);
        assert_eq!(p.visible_marks, 0);
        assert_eq!(p.active_stroke, None);
    }
}
