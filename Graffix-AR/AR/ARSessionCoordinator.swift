import ARKit
import CoreVideo
import RealityKit
import simd

/// Plain, `Sendable` plane data for the highlight overlay — extracted on the
/// AR queue so `ARPlaneAnchor` objects never cross actors.
struct PlaneRenderInfo: Sendable {
    var transform: simd_float4x4
    var center: SIMD3<Float>
    var extentWidth: Float
    var extentHeight: Float
    var rotationOnYAxis: Float
}

/// Feeds ARKit into the core engine, once per camera frame.
///
/// Runs on ARKit's session delegate queue (serial). It touches only thread-
/// safe state (`Engine` is internally synchronized, `sprayInput` is locked,
/// `planesByID` is confined to this queue) and hops to the main actor to
/// publish results.
final class ARSessionCoordinator: NSObject, ARSessionDelegate, @unchecked Sendable {
    private let engine: Engine
    private let sprayInput: SprayInput

    private weak var viewModel: GraffixViewModel?

    /// Stable u64 ids for ARPlaneAnchors (the core keys walls by u64).
    private var anchorIDs: [UUID: UInt64] = [:]
    private var nextAnchorID: UInt64 = 1
    /// Live plane render info by engine id — reused for the aim highlight.
    private(set) var planesByID: [UInt64: PlaneRenderInfo] = [:]

    /// Plane deltas consumed by the next `tick`.
    private var pendingUpserts: [PlaneObservationF] = []
    private var pendingRemovals: [UInt64] = []

    /// Previous frame timestamp, for the per-tick dt the core needs
    /// (hand speed, dwell stamping, shake decay).
    private var lastFrameTimestamp: TimeInterval?

    init(engine: Engine, sprayInput: SprayInput) {
        self.engine = engine
        self.sprayInput = sprayInput
        super.init()
    }

    @MainActor
    func attach(arView: ARView, viewModel: GraffixViewModel) {
        self.viewModel = viewModel
        viewModel.attach(renderer: WallCanvasRenderer(arView: arView))

        arView.session.delegate = self
        arView.automaticallyConfigureSession = false

        let config = ARWorldTrackingConfiguration()
        config.planeDetection = [.vertical]
        config.environmentTexturing = .automatic
        if ARWorldTrackingConfiguration.supportsFrameSemantics(.smoothedSceneDepth) {
            config.frameSemantics.insert(.smoothedSceneDepth)
        } else if ARWorldTrackingConfiguration.supportsFrameSemantics(.sceneDepth) {
            config.frameSemantics.insert(.sceneDepth)
        }
        arView.session.run(config)
    }

    // MARK: - ARSessionDelegate (session queue)

    func session(_ session: ARSession, didAdd anchors: [ARAnchor]) {
        for case let plane as ARPlaneAnchor in anchors where plane.alignment == .vertical {
            pendingUpserts.append(observation(from: plane))
        }
    }

    func session(_ session: ARSession, didUpdate anchors: [ARAnchor]) {
        for case let plane as ARPlaneAnchor in anchors where plane.alignment == .vertical {
            pendingUpserts.append(observation(from: plane))
        }
    }

    func session(_ session: ARSession, didRemove anchors: [ARAnchor]) {
        for case let plane as ARPlaneAnchor in anchors {
            if let id = anchorIDs[plane.identifier] {
                pendingRemovals.append(id)
                planesByID[id] = nil
                anchorIDs[plane.identifier] = nil
            }
        }
    }

    func session(_ session: ARSession, didUpdate frame: ARFrame) {
        let camera = frame.camera.transform
        let camPos = SIMD3<Float>(camera.columns.3.x, camera.columns.3.y, camera.columns.3.z)
        // ARKit camera looks down local -Z; forward = -(column 2).
        let forward = -SIMD3<Float>(camera.columns.2.x, camera.columns.2.y, camera.columns.2.z)

        let tracking: TrackingStateF = switch frame.camera.trackingState {
        case .normal: .normal
        case .limited: .limited
        case .notAvailable: .notAvailable
        }

        let dt = lastFrameTimestamp.map { frame.timestamp - $0 } ?? (1.0 / 60.0)
        lastFrameTimestamp = frame.timestamp

        let input = FrameInputF(
            cameraPosition: vec3(camPos),
            rayOrigin: vec3(camPos),
            rayDir: vec3(forward),
            planeUpserts: pendingUpserts,
            planeRemovals: pendingRemovals,
            lidarCenterDepthM: centerDepth(frame),
            tracking: tracking,
            spraying: sprayInput.value,
            dtSeconds: Float(dt)
        )
        pendingUpserts.removeAll(keepingCapacity: true)
        pendingRemovals.removeAll(keepingCapacity: true)

        let output = engine.tick(input: input)
        let planesSnapshot = planesByID
        Task { @MainActor [weak self] in
            guard let self, let viewModel = self.viewModel else { return }
            viewModel.apply(frameOutput: output)
            self.rendererUpdateHighlight(aimed: output.aimedWallId,
                                         locked: output.lockedWallId,
                                         planes: planesSnapshot)
        }
    }

    func session(_ session: ARSession, didFailWithError error: Error) {
        // Session interruption ends are handled by ARView; nothing to feed the core.
    }

    // MARK: - Plane conversion (pure data shuffling, no spray math)

    private func observation(from anchor: ARPlaneAnchor) -> PlaneObservationF {
        let id = anchorID(for: anchor.identifier)
        planesByID[id] = PlaneRenderInfo(
            transform: anchor.transform,
            center: anchor.center,
            extentWidth: Float(anchor.planeExtent.width),
            extentHeight: Float(anchor.planeExtent.height),
            rotationOnYAxis: anchor.planeExtent.rotationOnYAxis
        )

        let t = anchor.transform
        let center4 = t * SIMD4<Float>(anchor.center, 1)
        // Plane local +Y is the normal.
        let normal = simd_normalize(SIMD3<Float>(t.columns.1.x, t.columns.1.y, t.columns.1.z))
        // The extent rect is rotated `rotationOnYAxis` about the plane local Y;
        // send the matching "width" direction as axis_u.
        let theta = anchor.planeExtent.rotationOnYAxis
        let uLocal = SIMD3<Float>(cos(theta), 0, -sin(theta))
        let rot = simd_float3x3(
            SIMD3<Float>(t.columns.0.x, t.columns.0.y, t.columns.0.z),
            SIMD3<Float>(t.columns.1.x, t.columns.1.y, t.columns.1.z),
            SIMD3<Float>(t.columns.2.x, t.columns.2.y, t.columns.2.z)
        )
        let axisU = simd_normalize(rot * uLocal)

        return PlaneObservationF(
            id: id,
            center: vec3(SIMD3<Float>(center4.x, center4.y, center4.z)),
            normal: vec3(normal),
            axisU: vec3(axisU),
            halfExtentU: Float(anchor.planeExtent.width) / 2,
            halfExtentV: Float(anchor.planeExtent.height) / 2
        )
    }

    private func anchorID(for uuid: UUID) -> UInt64 {
        if let id = anchorIDs[uuid] { return id }
        let id = nextAnchorID
        nextAnchorID &+= 1
        anchorIDs[uuid] = id
        return id
    }

    // MARK: - Depth

    /// LiDAR depth at screen center, meters along the view axis — the same
    /// quantity as the core's geometric ray-plane `t`.
    private func centerDepth(_ frame: ARFrame) -> Float? {
        guard let map = frame.smoothedSceneDepth?.depthMap ?? frame.sceneDepth?.depthMap else {
            return nil
        }
        CVPixelBufferLockBaseAddress(map, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(map, .readOnly) }
        guard let base = CVPixelBufferGetBaseAddress(map) else { return nil }
        let w = CVPixelBufferGetWidth(map)
        let h = CVPixelBufferGetHeight(map)
        let stride = CVPixelBufferGetBytesPerRow(map) / MemoryLayout<Float32>.size
        let value = base.assumingMemoryBound(to: Float32.self)[(h / 2) * stride + w / 2]
        return (value.isFinite && value > 0) ? value : nil
    }

    // MARK: - Highlight

    /// Shows a translucent plate over the aimed wall (or the locked wall).
    @MainActor
    private func rendererUpdateHighlight(aimed: UInt64?, locked: UInt64?,
                                         planes: [UInt64: PlaneRenderInfo]) {
        guard let renderer = viewModel?.renderer else { return }
        let id = locked ?? aimed
        renderer.updateHighlight(plane: id.flatMap { planes[$0] }, locked: locked != nil)
    }
}

private func vec3(_ v: SIMD3<Float>) -> Vec3F {
    Vec3F(x: v.x, y: v.y, z: v.z)
}
