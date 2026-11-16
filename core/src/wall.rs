//! Wall registry: ingests plane observations from the shell, keeps track of
//! detected walls, and answers "which wall is the user aiming at?".

use std::collections::HashMap;

use crate::ffi::PlaneObservationF;
use crate::math::{Plane, PlaneFrame, Ray, EPS};

/// Extra slack (meters) around an observed wall extent that still counts as a
/// hit. ARKit under-estimates plane extents early on; without slack the aim
/// would flicker near perceived edges.
pub const AIM_MARGIN_M: f32 = 0.20;

#[derive(Clone, Debug)]
pub struct Wall {
    pub id: u64,
    /// Wall pose in world space (origin on the plane).
    pub frame: PlaneFrame,
    /// Half-extent along `u` (width) and `v` (height), meters.
    pub half_u: f32,
    pub half_v: f32,
}

impl Wall {
    pub fn plane(&self) -> Plane {
        self.frame.plane()
    }
}

/// Result of aiming the center ray into the registry.
#[derive(Clone, Copy, Debug)]
pub struct AimHit {
    pub wall_id: u64,
    /// Ray distance to the wall (spray travel distance), meters.
    pub t: f32,
}

#[derive(Default)]
pub struct WallRegistry {
    walls: HashMap<u64, Wall>,
}

impl WallRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or update a wall from a shell plane observation.
    ///
    /// `axis_u` is the wall's local "right" direction in world space; the core
    /// re-derives an orthonormal frame so small ARKit basis drift can't skew
    /// the dab coordinates.
    pub fn upsert(&mut self, obs: &PlaneObservationF) {
        let origin = crate::math::v3(obs.center);
        let normal = crate::math::v3(obs.normal).normalized();
        let axis_u = crate::math::v3(obs.axis_u);
        if !origin.is_finite() || normal.len() < EPS || axis_u.len() < EPS {
            return; // ignore malformed observations
        }
        let frame = PlaneFrame::new(origin, normal, axis_u);
        let wall = Wall {
            id: obs.id,
            frame,
            half_u: obs.half_extent_u.max(0.0),
            half_v: obs.half_extent_v.max(0.0),
        };
        self.walls.insert(obs.id, wall);
    }

    pub fn remove(&mut self, id: u64) {
        self.walls.remove(&id);
    }

    pub fn get(&self, id: u64) -> Option<&Wall> {
        self.walls.get(&id)
    }

    pub fn len(&self) -> usize {
        self.walls.len()
    }

    /// Nearest wall hit by `ray` within its (margined) extents.
    pub fn aim(&self, ray: Ray) -> Option<AimHit> {
        self.hit(ray, true)
    }

    /// Hit-test a single wall, ignoring its extent bounds (used for the locked
    /// wall so the user can spray slightly past the detected edge).
    pub fn hit_wall(&self, id: u64, ray: Ray) -> Option<AimHit> {
        let w = self.walls.get(&id)?;
        let t = w.plane().intersect_ray(ray)?;
        Some(AimHit { wall_id: id, t })
    }

    fn hit(&self, ray: Ray, bounded: bool) -> Option<AimHit> {
        let mut best: Option<AimHit> = None;
        for w in self.walls.values() {
            let Some(t) = w.plane().intersect_ray(ray) else {
                continue;
            };
            if bounded {
                let l = w.frame.to_local(ray.at(t));
                if l.x.abs() > w.half_u + AIM_MARGIN_M
                    || l.y.abs() > w.half_v + AIM_MARGIN_M
                {
                    continue;
                }
            }
            if best.is_none_or(|b| t < b.t) {
                best = Some(AimHit { wall_id: w.id, t });
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::{PlaneObservationF, Vec3F};
    use crate::math::Vec3;

    fn f3(x: f32, y: f32, z: f32) -> Vec3F {
        Vec3F { x, y, z }
    }

    /// A vertical wall 2 m ahead (z = 2), normal facing -z (toward camera).
    fn wall_obs(id: u64, center_z: f32) -> PlaneObservationF {
        PlaneObservationF {
            id,
            center: f3(0.0, 1.5, center_z),
            normal: f3(0.0, 0.0, -1.0),
            axis_u: f3(1.0, 0.0, 0.0),
            half_extent_u: 1.0,
            half_extent_v: 1.0,
        }
    }

    fn center_ray() -> Ray {
        Ray {
            origin: Vec3::new(0.0, 1.5, 0.0),
            dir: Vec3::new(0.0, 0.0, 1.0),
        }
    }

    #[test]
    fn upsert_and_aim_selects_nearest() {
        let mut r = WallRegistry::new();
        r.upsert(&wall_obs(1, 2.0));
        r.upsert(&wall_obs(2, 1.0));
        let hit = r.aim(center_ray()).unwrap();
        assert_eq!(hit.wall_id, 2);
        assert!((hit.t - 1.0).abs() < 1e-4);
    }

    #[test]
    fn aim_respects_extents() {
        let mut r = WallRegistry::new();
        r.upsert(&wall_obs(1, 2.0));
        // Ray aimed far right of the 1 m half-extent misses.
        let miss = Ray {
            origin: Vec3::new(0.0, 1.5, 0.0),
            dir: Vec3::new(5.0, 0.0, 2.0).normalized(),
        };
        assert!(r.aim(miss).is_none());
        // Just outside the extent but inside the margin still hits.
        let edge = Ray {
            origin: Vec3::new(0.0, 1.5, 0.0),
            dir: Vec3::new(1.05, 0.0, 2.0).normalized(),
        };
        // hit x = 1.05 * (2/2.028...) * ... ~ x > 1.0 < 1.2 -> margin hit
        assert!(r.aim(edge).is_some());
    }

    #[test]
    fn lock_selection_flow() {
        let mut r = WallRegistry::new();
        r.upsert(&wall_obs(7, 2.0));
        let aimed = r.aim(center_ray()).map(|h| h.wall_id);
        assert_eq!(aimed, Some(7));
        // Locked wall hit ignores bounds.
        let far = Ray {
            origin: Vec3::new(0.0, 1.5, 0.0),
            dir: Vec3::new(4.0, 0.0, 2.0).normalized(),
        };
        assert!(r.aim(far).is_none());
        assert!(r.hit_wall(7, far).is_some());
        // Removal clears it.
        r.remove(7);
        assert!(r.aim(center_ray()).is_none());
    }
}
