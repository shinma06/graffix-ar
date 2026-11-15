//! Minimal vector / plane math. All spray-relevant geometry lives here so the
//! shell never has to do projection math itself.
//!
//! Units are meters, right-handed world space (ARKit convention).

use crate::ffi::{Vec2F, Vec3F};

pub const EPS: f32 = 1e-6;

#[inline]
pub fn v3(v: Vec3F) -> Vec3 {
    Vec3 {
        x: v.x,
        y: v.y,
        z: v.z,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub const UP: Vec3 = Vec3 {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }

    pub fn normalized(self) -> Vec3 {
        let l = self.len();
        if l < EPS {
            Vec3::ZERO
        } else {
            self * (1.0 / l)
        }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    /// Component of `self` orthogonal to unit vector `n`.
    pub fn perpendicular_to(self, n: Vec3) -> Vec3 {
        self - n * self.dot(n)
    }
}

impl From<Vec3> for Vec3F {
    fn from(v: Vec3) -> Self {
        Vec3F {
            x: v.x,
            y: v.y,
            z: v.z,
        }
    }
}

impl std::ops::Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl std::ops::Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl std::ops::Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}
impl std::ops::Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f32) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn len(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
}

impl From<Vec2> for Vec2F {
    fn from(v: Vec2) -> Self {
        Vec2F { x: v.x, y: v.y }
    }
}
impl std::ops::Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}
impl std::ops::Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}
impl std::ops::Mul<f32> for Vec2 {
    type Output = Vec2;
    fn mul(self, s: f32) -> Vec2 {
        Vec2::new(self.x * s, self.y * s)
    }
}

/// Ray with (expected) unit direction.
#[derive(Clone, Copy, Debug)]
pub struct Ray {
    pub origin: Vec3,
    pub dir: Vec3,
}

impl Ray {
    pub fn at(self, t: f32) -> Vec3 {
        self.origin + self.dir * t
    }
}

/// Infinite plane: `point` on the plane, unit `normal`.
#[derive(Clone, Copy, Debug)]
pub struct Plane {
    pub point: Vec3,
    pub normal: Vec3,
}

impl Plane {
    /// Signed distance from `p` to the plane (positive on the normal side).
    #[allow(dead_code)] // part of the geometry API; exercised by tests today
    pub fn signed_distance(&self, p: Vec3) -> f32 {
        (p - self.point).dot(self.normal)
    }

    /// Nearest point on the plane to `p`.
    #[allow(dead_code)] // part of the geometry API; exercised by tests today
    pub fn project(&self, p: Vec3) -> Vec3 {
        p - self.normal * self.signed_distance(p)
    }

    /// Distance `t` along the ray where it crosses the plane.
    /// `None` when the ray is (near-)parallel or the hit is behind the origin.
    pub fn intersect_ray(&self, ray: Ray) -> Option<f32> {
        let denom = ray.dir.dot(self.normal);
        if denom.abs() < EPS {
            return None;
        }
        let t = (self.point - ray.origin).dot(self.normal) / denom;
        (t >= 0.0).then_some(t)
    }
}

/// Orthonormal 2D frame on a plane: `u` (local x), `v` (local y),
/// `normal = u x v`. Wall-local dab coordinates live in this frame.
#[derive(Clone, Copy, Debug)]
pub struct PlaneFrame {
    pub origin: Vec3,
    pub u: Vec3,
    pub v: Vec3,
    pub normal: Vec3,
}

impl PlaneFrame {
    /// Build a frame from a normal and a preferred "right" axis.
    /// `axis_u` is re-orthogonalized against `normal`.
    pub fn new(origin: Vec3, normal: Vec3, axis_u: Vec3) -> Self {
        let normal = normal.normalized();
        let u = axis_u.perpendicular_to(normal).normalized();
        // Right-handedness: u x v = normal  =>  v = normal x u.
        let v = normal.cross(u).normalized();
        Self {
            origin,
            u,
            v,
            normal,
        }
    }

    /// Canonical drawing frame for a wall: the normal is flipped to face
    /// `toward` (the camera), and `v` is chosen as close to world-up as the
    /// wall allows so strokes aren't sideways/upside-down.
    pub fn facing(plane: Plane, toward: Vec3, world_up: Vec3) -> Self {
        let mut normal = plane.normal.normalized();
        if normal.dot(toward - plane.point) < 0.0 {
            normal = -normal;
        }
        let mut v = world_up.perpendicular_to(normal);
        if v.len() < 1e-3 {
            // Degenerate (floor/ceiling): pick any horizontal axis.
            v = Vec3::new(1.0, 0.0, 0.0).perpendicular_to(normal);
        }
        let v = v.normalized();
        // u x v = normal => u = v x normal.
        let u = v.cross(normal).normalized();
        Self {
            origin: plane.point,
            u,
            v,
            normal,
        }
    }

    /// World point -> local (u, v) plane coordinates, in meters.
    pub fn to_local(self, p: Vec3) -> Vec2 {
        let d = p - self.origin;
        Vec2::new(d.dot(self.u), d.dot(self.v))
    }

    /// Local (u, v) plane coordinates -> world point.
    #[allow(dead_code)] // part of the frame API; exercised by tests today
    #[allow(clippy::wrong_self_convention)]
    pub fn from_local(self, l: Vec2) -> Vec3 {
        self.origin + self.u * l.x + self.v * l.y
    }

    pub fn plane(&self) -> Plane {
        Plane {
            point: self.origin,
            normal: self.normal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ray(o: Vec3, d: Vec3) -> Ray {
        Ray {
            origin: o,
            dir: d.normalized(),
        }
    }

    #[test]
    fn ray_plane_head_on() {
        let p = Plane {
            point: Vec3::new(0.0, 0.0, 2.0),
            normal: Vec3::new(0.0, 0.0, -1.0),
        };
        let r = ray(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(p.intersect_ray(r), Some(2.0));
    }

    #[test]
    fn ray_plane_angled() {
        // Wall at x = 1. Ray from origin along (1,0,0) tilted 45 deg stays t=1/cos.
        let p = Plane {
            point: Vec3::new(1.0, 0.0, 0.0),
            normal: Vec3::new(-1.0, 0.0, 0.0),
        };
        let r = ray(Vec3::ZERO, Vec3::new(1.0, 1.0, 0.0));
        let t = p.intersect_ray(r).unwrap();
        assert!((t - 2f32.sqrt()).abs() < 1e-4);
        let hit = r.at(t);
        assert!((hit.x - 1.0).abs() < 1e-4);
    }

    #[test]
    fn ray_plane_parallel_and_behind() {
        let p = Plane {
            point: Vec3::ZERO,
            normal: Vec3::UP,
        };
        // Parallel ray: no hit.
        assert_eq!(
            p.intersect_ray(ray(Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 0.0, 0.0))),
            None
        );
        // Plane behind the camera: no hit.
        assert_eq!(
            p.intersect_ray(ray(Vec3::new(0.0, 1.0, 0.0), Vec3::UP)),
            None
        );
    }

    #[test]
    fn point_plane_signed_distance() {
        let p = Plane {
            point: Vec3::new(0.0, 0.0, 2.0),
            normal: Vec3::new(0.0, 0.0, -1.0),
        };
        // Camera at origin, plane 2 m ahead facing the camera.
        assert!((p.signed_distance(Vec3::ZERO) - 2.0).abs() < 1e-6);
        assert!((p.signed_distance(Vec3::new(0.0, 0.0, 5.0)) + 3.0).abs() < 1e-6);
        // Projection lands on the plane.
        let q = p.project(Vec3::new(1.0, 2.0, 0.0));
        assert!(p.signed_distance(q).abs() < 1e-5);
        assert!((q.z - 2.0).abs() < 1e-6);
    }

    #[test]
    fn frame_round_trip() {
        let f = PlaneFrame::facing(
            Plane {
                point: Vec3::new(0.0, 0.0, 3.0),
                normal: Vec3::new(0.0, 0.0, -1.0),
            },
            Vec3::ZERO,
            Vec3::UP,
        );
        let world = f.from_local(Vec2::new(0.4, 1.2));
        let back = f.to_local(world);
        assert!((back.x - 0.4).abs() < 1e-5 && (back.y - 1.2).abs() < 1e-5);
        // v should be world-up for a vertical wall.
        assert!((f.v.y - 1.0).abs() < 1e-5);
        // u x v == normal, and normal faces the camera.
        let n = f.u.cross(f.v);
        assert!((n - f.normal).len() < 1e-5);
        assert!(f.normal.z < 0.0);
    }
}
