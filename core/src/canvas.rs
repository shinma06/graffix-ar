//! Strokes, dabs, undo/redo. All coordinates are wall-local meters.

use crate::ffi::{ColorF, DabF, DripF, StrokeF};
use crate::math::{PlaneFrame, Vec2};

/// A single spray stamp on the wall (model v2): an ellipse with a density
/// gradient toward the near side. Head-on real-mode dabs and all casual
/// dabs are circles (`radius_major == radius_minor`, zero `gradient`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dab {
    pub uv: Vec2,
    /// Ellipse semi-axes, meters. Major lies along the spray tilt direction.
    pub radius_major: f32,
    pub radius_minor: f32,
    /// Major-axis angle in wall-local radians.
    pub rotation: f32,
    /// Unit vector toward the denser (near) side; zero when head-on.
    pub gradient: Vec2,
    pub opacity: f32,
    /// Edge softness 0..1 (0 = crisp marker edge, 1 = very soft halo).
    pub softness: f32,
}

impl Dab {
    /// A simple soft-disc dab (v1-equivalent shape).
    pub fn circle(uv: Vec2, radius: f32, opacity: f32, softness: f32) -> Self {
        Self {
            uv,
            radius_major: radius,
            radius_minor: radius,
            rotation: 0.0,
            gradient: Vec2::new(0.0, 0.0),
            opacity,
            softness,
        }
    }
}

/// A drip streak: paint that saturated one spot and ran down the wall.
/// Runs from `uv` straight down wall-local -v for `len` meters. Drips are
/// part of the canvas model (serialized with the work), not a render effect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drip {
    /// Head of the streak (top), wall-local.
    pub uv: Vec2,
    /// Run length downward, meters.
    pub len: f32,
    /// Streak width, meters.
    pub width: f32,
    pub opacity: f32,
}

/// A continuous stroke painted in one palette color.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub palette_index: u8,
    pub dabs: Vec<Dab>,
    /// Drips spawned by this stroke's oversaturation (undo removes them
    /// with the stroke).
    pub drips: Vec<Drip>,
    /// When each mark was painted (timelapse replay). `None` for strokes
    /// loaded from pre-v3 data, which carried no timing.
    pub timing: Option<StrokeTiming>,
}

/// Paint timing of one stroke, in milliseconds of canvas time (the engine's
/// accumulated frame time, so idle app time between ticks is not counted).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StrokeTiming {
    /// Canvas time at which the stroke began.
    pub start_ms: u32,
    /// Per dab, offset from `start_ms` (non-decreasing, same length as `dabs`).
    pub dab_ms: Vec<u32>,
    /// Per drip, offset from `start_ms` (non-decreasing, same length as `drips`).
    pub drip_ms: Vec<u32>,
}

impl StrokeTiming {
    /// Canvas time of the stroke's last recorded mark.
    pub fn end_ms(&self) -> u32 {
        let last = self
            .dab_ms
            .last()
            .copied()
            .unwrap_or(0)
            .max(self.drip_ms.last().copied().unwrap_or(0));
        self.start_ms.saturating_add(last)
    }
}

/// One reversible edit.
#[derive(Clone, Debug)]
pub enum Edit {
    /// A stroke was appended; undo removes `stroke` from `index`.
    Paint { index: usize, stroke: Stroke },
    /// The canvas was cleared; undo restores `strokes`.
    Cleared { strokes: Vec<Stroke> },
}

#[derive(Default)]
pub struct Canvas {
    /// Frozen wall frame captured at lock time. Dab coordinates are stable in
    /// this frame even if ARKit later refines the wall pose, and this is the
    /// frame that gets serialized for future re-anchoring.
    frame: Option<PlaneFrame>,
    strokes: Vec<Stroke>,
    undo_stack: Vec<Edit>,
    redo_stack: Vec<Edit>,
    /// Monotonic counter; the shell treats growth as "needs re-render".
    revision: u64,
}

impl Canvas {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn frame(&self) -> Option<PlaneFrame> {
        self.frame
    }

    /// Capture the canvas frame (once per canvas). Called when a wall locks.
    pub fn set_frame(&mut self, frame: PlaneFrame) {
        if self.frame.is_none() {
            self.frame = Some(frame);
            self.bump();
        }
    }

    pub fn strokes(&self) -> &[Stroke] {
        &self.strokes
    }

    #[allow(dead_code)] // used by tests; shell uses `CanvasSnapshotF.dab_count`
    pub fn dab_count(&self) -> usize {
        self.strokes.iter().map(|s| s.dabs.len()).sum()
    }

    /// Append a stroke and record it as an undoable edit.
    /// Empty strokes are dropped silently.
    pub fn commit_stroke(&mut self, stroke: Stroke) {
        if stroke.dabs.is_empty() {
            return;
        }
        let index = self.strokes.len();
        self.undo_stack.push(Edit::Paint {
            index,
            stroke: stroke.clone(),
        });
        self.strokes.push(stroke);
        self.redo_stack.clear();
        self.bump();
    }

    pub fn clear(&mut self) {
        if self.strokes.is_empty() {
            return;
        }
        let strokes = std::mem::take(&mut self.strokes);
        self.undo_stack.push(Edit::Cleared { strokes });
        self.redo_stack.clear();
        self.bump();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(edit) = self.undo_stack.pop() else {
            return false;
        };
        match edit {
            Edit::Paint { index, stroke } => {
                if index < self.strokes.len() {
                    self.strokes.remove(index);
                }
                self.redo_stack.push(Edit::Paint { index, stroke });
            }
            Edit::Cleared { strokes } => {
                let current = std::mem::replace(&mut self.strokes, strokes);
                debug_assert!(current.is_empty());
                self.redo_stack.push(Edit::Cleared {
                    strokes: current,
                });
            }
        }
        self.bump();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(edit) = self.redo_stack.pop() else {
            return false;
        };
        match edit {
            Edit::Paint { index, stroke } => {
                self.strokes.insert(index.min(self.strokes.len()), stroke.clone());
                self.undo_stack.push(Edit::Paint { index, stroke });
            }
            Edit::Cleared { strokes } => {
                let current = std::mem::take(&mut self.strokes);
                self.undo_stack.push(Edit::Cleared { strokes: current });
                let _ = strokes;
            }
        }
        self.bump();
        true
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn bump(&mut self) {
        self.revision += 1;
    }

    /// Load a deserialized canvas (used by `Engine::load_canvas`).
    pub fn replace(&mut self, frame: Option<PlaneFrame>, strokes: Vec<Stroke>) {
        self.frame = frame;
        self.strokes = strokes;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.bump();
    }

    /// Convert to the FFI snapshot type, resolving palette colors.
    pub fn snapshot(&self, resolve_color: impl Fn(u8) -> ColorF) -> (Option<PlaneFrame>, Vec<StrokeF>) {
        let strokes = self
            .strokes
            .iter()
            .map(|s| StrokeF {
                palette_index: s.palette_index,
                color: resolve_color(s.palette_index),
                dabs: s.dabs.iter().map(dab_f).collect(),
                drips: s.drips.iter().map(drip_f).collect(),
            })
            .collect();
        (self.frame, strokes)
    }
}

/// FFI mapping for a single dab (shared by `Canvas::snapshot` and the
/// in-flight stroke preview in `Engine::canvas_snapshot`).
pub(crate) fn dab_f(d: &Dab) -> DabF {
    DabF {
        u: d.uv.x,
        v: d.uv.y,
        radius_major_m: d.radius_major,
        radius_minor_m: d.radius_minor,
        rotation_rad: d.rotation,
        gradient_u: d.gradient.x,
        gradient_v: d.gradient.y,
        opacity: d.opacity,
        softness: d.softness,
    }
}

/// FFI mapping for a drip streak.
pub(crate) fn drip_f(d: &Drip) -> DripF {
    DripF {
        u: d.uv.x,
        v: d.uv.y,
        len_m: d.len,
        width_m: d.width,
        opacity: d.opacity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Vec2;

    fn stroke(n: usize) -> Stroke {
        Stroke {
            palette_index: 1,
            dabs: (0..n)
                .map(|i| Dab::circle(Vec2::new(i as f32 * 0.01, 0.0), 0.05, 0.8, 0.5))
                .collect(),
            drips: Vec::new(),
            timing: None,
        }
    }

    #[test]
    fn undo_redo_round_trip() {
        let mut c = Canvas::new();
        c.commit_stroke(stroke(3));
        c.commit_stroke(stroke(5));
        assert_eq!(c.dab_count(), 8);
        assert!(c.undo());
        assert_eq!(c.dab_count(), 3);
        assert!(c.redo());
        assert_eq!(c.dab_count(), 8);
        assert!(c.undo());
        assert!(c.undo());
        assert!(!c.can_undo());
        assert!(!c.undo());
    }

    #[test]
    fn clear_is_undoable_and_new_edit_clears_redo() {
        let mut c = Canvas::new();
        c.commit_stroke(stroke(2));
        c.clear();
        assert_eq!(c.dab_count(), 0);
        assert!(c.undo()); // restore cleared strokes
        assert_eq!(c.dab_count(), 2);
        assert!(c.can_redo());
        // A new edit invalidates redo.
        c.commit_stroke(stroke(1));
        assert!(!c.can_redo());
    }

    #[test]
    fn empty_stroke_is_not_committed() {
        let mut c = Canvas::new();
        c.commit_stroke(Stroke {
            palette_index: 0,
            dabs: vec![],
            drips: vec![],
            timing: None,
        });
        assert!(!c.can_undo());
        assert_eq!(c.dab_count(), 0);
    }
}
