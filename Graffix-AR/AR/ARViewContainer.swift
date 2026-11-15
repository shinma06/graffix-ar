import ARKit
import RealityKit
import SwiftUI

/// RealityKit AR surface for the app.
///
/// Why `ARView` (UIViewRepresentable) and not `RealityView`+`.spatialTracking`:
/// on iOS only one AR session may run, and `SpatialTrackingSession` does not
/// expose the inputs the core needs — enumerated `ARPlaneAnchor`s, per-frame
/// camera pose, or `sceneDepth`. `ARView` is the RealityKit view backed by a
/// real `ARSession`, so it supplies all three while still rendering into
/// SwiftUI. Isolated to this file: migrating to RealityView later only means
/// re-sourcing the frame/plane/depth feed, since all math is in the core.
struct ARViewContainer: UIViewRepresentable {
    let engine: Engine
    let sprayInput: SprayInput
    let viewModel: GraffixViewModel

    func makeCoordinator() -> ARSessionCoordinator {
        ARSessionCoordinator(engine: engine, sprayInput: sprayInput)
    }

    func makeUIView(context: Context) -> ARView {
        let arView = ARView(frame: .zero)
        context.coordinator.attach(arView: arView, viewModel: viewModel)
        return arView
    }

    func updateUIView(_ uiView: ARView, context: Context) {}
}
