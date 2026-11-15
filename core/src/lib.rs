//! graffix-core: the Rust engine behind Shibuya Graffix AR.
//!
//! The shell (SwiftUI + RealityKit) feeds ARKit plane/camera/depth input via
//! `Engine::tick` and renders what the core returns. All spray math, wall
//! selection, distance estimation and canvas serialization live here.

uniffi::setup_scaffolding!();

mod canvas;
mod distance;
pub mod engine;
pub mod ffi;
mod math;
mod palette;
mod serialize;
mod spray;
mod wall;

use canvas::Canvas;

impl Canvas {
    /// Bump the revision when dabs are appended to the in-flight stroke (which
    /// lives outside `self.strokes` until the stroke commits).
    pub(crate) fn bump_dabs(&mut self) {
        self.bump();
    }
}
