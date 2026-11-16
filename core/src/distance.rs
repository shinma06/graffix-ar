//! Camera->wall distance estimate.
//!
//! Two sources feed the estimate:
//!   * geometric: ray-plane intersection distance `t` along the center ray —
//!     i.e. how far paint actually travels before hitting the wall;
//!   * lidar: a scene-depth sample at screen center, supplied by the shell on
//!     devices that have it (same quantity: distance along the view ray).
//!
//! When both exist and agree, the sample is fused and reliability rises; when
//! they disagree we trust geometry (it is exact w.r.t. the tracked plane) and
//! drop reliability. The result is exponentially smoothed so the spray
//! footprint doesn't jitter frame to frame.

/// Base reliability contributed by each tracking state.
fn tracking_base(t: Tracking) -> f32 {
    match t {
        Tracking::Normal => 1.0,
        Tracking::Limited => 0.55,
        Tracking::NotAvailable => 0.15,
    }
}

/// Simplified ARKit tracking state supplied by the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tracking {
    Normal,
    Limited,
    NotAvailable,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistanceEstimate {
    /// Smoothed camera->wall distance along the view ray, meters.
    pub distance: Option<f32>,
    /// 0.0..1.0 — how much the spray model should trust `distance`.
    pub reliability: f32,
    /// Which source produced the estimate.
    pub fused_lidar: bool,
}

#[derive(Clone, Debug)]
pub struct DistanceFilter {
    /// EMA factor: higher = more responsive, lower = smoother.
    alpha: f32,
    smoothed: Option<f32>,
}

impl Default for DistanceFilter {
    fn default() -> Self {
        Self {
            alpha: 0.35,
            smoothed: None,
        }
    }
}

impl DistanceFilter {
    pub fn reset(&mut self) {
        self.smoothed = None;
    }

    /// Feed one frame. `geometric` is the ray-plane distance for the wall in
    /// use (locked or aimed); `lidar` is the center depth sample if present.
    pub fn update(
        &mut self,
        geometric: Option<f32>,
        lidar: Option<f32>,
        tracking: Tracking,
    ) -> DistanceEstimate {
        let base = tracking_base(tracking);

        let (raw, reliability, fused_lidar) = match (geometric, lidar) {
            (Some(g), Some(l)) if g.is_finite() && l.is_finite() && l > 0.0 => {
                // Agreement test: LiDAR within 15% (min 8 cm) of geometry.
                let tol = (0.15 * g).max(0.08);
                let agree = ((l - g).abs() <= tol) as u8 as f32;
                // Blend toward the mean on agreement; distrust on disagreement.
                let fused = if agree > 0.5 { (g + l) * 0.5 } else { g };
                (
                    fused,
                    base * (0.55 + 0.45 * agree),
                    agree > 0.5,
                )
            }
            (Some(g), _) if g.is_finite() && g > 0.0 => (g, base * 0.85, false),
            (_, Some(l)) if l.is_finite() && l > 0.0 => (l, base * 0.6, false),
            _ => (f32::NAN, 0.0, false),
        };

        if !raw.is_finite() {
            return DistanceEstimate {
                distance: self.smoothed,
                reliability,
                fused_lidar: false,
            };
        }

        let d = match self.smoothed {
            Some(s) => s + self.alpha * (raw - s),
            None => raw,
        };
        self.smoothed = Some(d);
        DistanceEstimate {
            distance: Some(d),
            reliability: reliability.clamp(0.0, 1.0),
            fused_lidar,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometric_only_smoothing() {
        let mut f = DistanceFilter::default();
        let e1 = f.update(Some(1.0), None, Tracking::Normal);
        assert_eq!(e1.distance, Some(1.0));
        // Jumpy input gets smoothed toward the new value, not snapped.
        let e2 = f.update(Some(2.0), None, Tracking::Normal);
        let d = e2.distance.unwrap();
        assert!(d > 1.0 && d < 2.0);
        assert!((d - 1.35).abs() < 1e-4);
    }

    #[test]
    fn lidar_agreement_raises_reliability() {
        let mut f = DistanceFilter::default();
        let agree = f.update(Some(1.0), Some(1.02), Tracking::Normal);
        let mut g = DistanceFilter::default();
        let disagree = g.update(Some(1.0), Some(2.0), Tracking::Normal);
        assert!(agree.reliability > disagree.reliability);
        assert!(agree.fused_lidar);
        assert!(!disagree.fused_lidar);
        // On disagreement we keep the geometric distance.
        assert!((disagree.distance.unwrap() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn tracking_state_scales_reliability() {
        let mut f = DistanceFilter::default();
        let normal = f.update(Some(1.0), None, Tracking::Normal);
        let mut g = DistanceFilter::default();
        let lost = g.update(Some(1.0), None, Tracking::NotAvailable);
        assert!(normal.reliability > lost.reliability);
    }

    #[test]
    fn reset_clears_smoothing() {
        let mut f = DistanceFilter::default();
        f.update(Some(1.0), None, Tracking::Normal);
        f.reset();
        let e = f.update(Some(3.0), None, Tracking::Normal);
        assert_eq!(e.distance, Some(3.0));
    }
}
