import ARKit
import CoreGraphics
import RealityKit
import UIKit

/// Lays the core's live canvas texture (`Engine.renderLiveCanvas()`) on a
/// plane at the frozen canvas frame, plus the aim/lock wall highlight.
///
/// The core rasterizes the paint with the same rasterizer as saved-work
/// previews, timelapse and video export, incrementally (only new marks are
/// stamped). This class only uploads the returned RGBA to a RealityKit
/// texture and positions one quad over the wall rectangle the image covers —
/// one draw item regardless of dab count, crisp at any viewing angle.
@MainActor
final class WallCanvasRenderer {
    private weak var arView: ARView?
    private var canvasEntity: ModelEntity?
    private var canvasTexture: TextureResource?
    private var canvasPlacement: CanvasPlacement?
    private var highlightEntity: ModelEntity?

    /// Core render running off the main actor; at most one at a time.
    private var renderTask: Task<Void, Never>?
    /// The canvas changed while `renderTask` ran; render again afterwards.
    private var renderPending = false
    private var highlightSizeKey: (Int, Int) = (-1, -1)
    private var highlightLocked = false

    init(arView: ARView) {
        self.arView = arView
    }

    /// 撮影: カメラ映像込みの ARView スナップショット。
    func snapshot() async -> UIImage? {
        guard let arView else { return nil }
        return await withCheckedContinuation { cont in
            arView.snapshot(saveToHDR: false) { image in
                cont.resume(returning: image)
            }
        }
    }

    // MARK: - Canvas texture

    /// Re-render the canvas texture off the main actor. Calls arriving while a
    /// render runs coalesce into one follow-up render.
    func requestCanvasRender(engine: Engine) {
        guard renderTask == nil else {
            renderPending = true
            return
        }
        renderTask = Task { [weak self] in
            let image = await Task.detached(priority: .userInitiated) {
                engine.renderLiveCanvas()
            }.value
            guard let self else { return }
            self.renderTask = nil
            self.present(image)
            if self.renderPending {
                self.renderPending = false
                self.requestCanvasRender(engine: engine)
            }
        }
    }

    private func present(_ image: LiveCanvasImageF?) {
        guard let arView, let image, let cgImage = Self.cgImage(image) else {
            removeCanvas()
            return
        }
        let placement = CanvasPlacement(image)
        let options = TextureResource.CreateOptions(semantic: .color)

        if placement == canvasPlacement, canvasEntity != nil, let texture = canvasTexture,
           (try? texture.replace(withImage: cgImage, options: options)) != nil {
            return // same quad, pixels updated in place
        }
        guard let texture = try? TextureResource(image: cgImage, withName: nil, options: options)
        else { return }

        var material = UnlitMaterial()
        material.color = .init(tint: .white, texture: .init(texture))
        material.blending = .transparent(opacity: 1.0)
        canvasTexture = texture

        if placement == canvasPlacement, let entity = canvasEntity {
            entity.model?.materials = [material]
            return
        }

        // Entity sits on the wall: local X -> canvas u (right), local Y ->
        // canvas normal (out of the wall), local Z -> -v so that +v is "up"
        // in texture space (image row 0 is the +v edge).
        let frame = image.frame
        let centerU = image.minU + image.widthM / 2
        let centerV = image.minV + image.heightM / 2
        let u = simd(frame.axisU), v = simd(frame.axisV), n = simd(frame.normal)
        let origin = simd(frame.origin) + u * centerU + v * centerV
        var m = matrix_identity_float4x4
        m.columns.0 = SIMD4<Float>(u, 0)
        m.columns.1 = SIMD4<Float>(n, 0)
        m.columns.2 = SIMD4<Float>(-v, 0)
        m.columns.3 = SIMD4<Float>(origin, 1)

        let entity = ModelEntity(mesh: .generatePlane(width: image.widthM, depth: image.heightM),
                                 materials: [material])
        entity.setTransformMatrix(m, relativeTo: nil)

        canvasEntity?.parent?.removeFromParent()
        let anchor = AnchorEntity(world: matrix_identity_float4x4)
        anchor.addChild(entity)
        arView.scene.addAnchor(anchor)
        canvasEntity = entity
        canvasPlacement = placement
    }

    func removeCanvas() {
        canvasEntity?.parent?.removeFromParent()
        canvasEntity = nil
        canvasTexture = nil
        canvasPlacement = nil
    }

    /// Wraps the core's premultiplied sRGB RGBA8 (top-left origin) for upload.
    private static func cgImage(_ image: LiveCanvasImageF) -> CGImage? {
        let width = Int(image.width), height = Int(image.height)
        guard width > 0, height > 0, image.rgba.count == width * height * 4,
              let provider = CGDataProvider(data: image.rgba as CFData),
              let space = CGColorSpace(name: CGColorSpace.sRGB)
        else { return nil }
        return CGImage(
            width: width, height: height,
            bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4,
            space: space,
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
            provider: provider, decode: nil,
            shouldInterpolate: true, intent: .defaultIntent)
    }

    // MARK: - Aim/lock highlight

    func updateHighlight(plane: PlaneRenderInfo?, locked: Bool) {
        guard let arView, let plane else {
            highlightEntity?.parent?.removeFromParent()
            highlightEntity = nil
            return
        }
        let entity: ModelEntity
        if let existing = highlightEntity {
            entity = existing
        } else {
            entity = ModelEntity()
            let anchorEntity = AnchorEntity(world: matrix_identity_float4x4)
            anchorEntity.addChild(entity)
            arView.scene.addAnchor(anchorEntity)
            highlightEntity = entity
        }
        let color: UIColor = locked ? .systemGreen.withAlphaComponent(0.18)
                                    : .systemCyan.withAlphaComponent(0.15)
        // Rebuild the mesh only when the detected extent actually changes.
        let sizeKey = (Int(plane.extentWidth * 100), Int(plane.extentHeight * 100))
        if sizeKey != highlightSizeKey || entity.model == nil {
            entity.model = .init(
                mesh: .generatePlane(width: plane.extentWidth,
                                     depth: plane.extentHeight),
                materials: [UnlitMaterial(color: color)]
            )
            highlightSizeKey = sizeKey
        } else if locked != highlightLocked {
            entity.model?.materials = [UnlitMaterial(color: color)]
        }
        highlightLocked = locked
        // Extent rect: centered at anchor center, yawed by rotationOnYAxis
        // in plane-local space, then the anchor's world transform.
        let theta = plane.rotationOnYAxis
        var local = matrix_identity_float4x4
        local.columns.0 = SIMD4<Float>(cos(theta), 0, -sin(theta), 0)
        local.columns.2 = SIMD4<Float>(sin(theta), 0, cos(theta), 0)
        local.columns.3 = SIMD4<Float>(plane.center, 1)
        entity.setTransformMatrix(plane.transform * local, relativeTo: nil)
    }
}

/// Where a canvas quad sits: same placement -> reuse the entity.
private struct CanvasPlacement: Equatable {
    let frame: CanvasFrameF
    let minU: Float, minV: Float, widthM: Float, heightM: Float, width: UInt32, height: UInt32

    init(_ image: LiveCanvasImageF) {
        frame = image.frame
        minU = image.minU
        minV = image.minV
        widthM = image.widthM
        heightM = image.heightM
        width = image.width
        height = image.height
    }
}

private func simd(_ v: Vec3F) -> SIMD3<Float> {
    SIMD3<Float>(v.x, v.y, v.z)
}
