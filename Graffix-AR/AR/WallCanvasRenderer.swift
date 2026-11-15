import ARKit
import CoreGraphics
import RealityKit
import UIKit

/// Renders the core's canvas snapshot as a texture stamped onto a plane at
/// the frozen canvas frame — the "texture-from-dabs" strategy.
///
/// Rationale: strokes are already resolution-independent geometry (position +
/// radius + opacity), so rasterizing them into a wall-aligned texture is the
/// cheapest faithful rendering — one quad, one texture, trivially crisp at
/// any viewing angle. The alternative (per-dab entities/billboards) costs one
/// draw item per stamp and gets expensive fast.
@MainActor
final class WallCanvasRenderer {
    /// Texture density: pixels per meter of wall.
    static let pixelsPerMeter: CGFloat = 256
    /// Padding around dab bounds, meters.
    static let canvasPad: Float = 0.10
    static let maxTextureSide = 4096

    private weak var arView: ARView?
    private var canvasEntity: ModelEntity?
    private var highlightEntity: ModelEntity?

    /// Persistent bitmap the dabs accumulate into. Dabs and drips are
    /// append-only (undo/clear trigger a full reset), so a live stroke only
    /// needs to draw the newly appended marks instead of re-rasterizing
    /// everything.
    private var bitmap: CGContext?
    private var bitmapBounds: (minU: Float, minV: Float, widthM: Float, heightM: Float)?
    private var renderedFlatCount = 0
    private var renderedDripCount = 0
    private var renderedStrokeCount = 0
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

    func update(canvas snapshot: CanvasSnapshotF) {
        guard let arView else { return }
        guard let frame = snapshot.frame, snapshot.dabCount > 0 else {
            removeCanvas()
            return
        }

        // Wall-local bounds over all dabs and drip streaks.
        var minU = Float.greatestFiniteMagnitude
        var maxU = -Float.greatestFiniteMagnitude
        var minV = minU
        var maxV = maxU
        for stroke in snapshot.strokes {
            for dab in stroke.dabs {
                let r = max(dab.radiusMajorM, dab.radiusMinorM)
                minU = min(minU, dab.u - r)
                maxU = max(maxU, dab.u + r)
                minV = min(minV, dab.v - r)
                maxV = max(maxV, dab.v + r)
            }
            for drip in stroke.drips {
                let w = drip.widthM * 0.5
                minU = min(minU, drip.u - w)
                maxU = max(maxU, drip.u + w)
                minV = min(minV, drip.v - drip.lenM)
                maxV = max(maxV, drip.v + w)
            }
        }
        minU -= Self.canvasPad
        minV -= Self.canvasPad
        maxU += Self.canvasPad
        maxV += Self.canvasPad

        let widthM = max(maxU - minU, 0.05)
        let heightM = max(maxV - minV, 0.05)

        let dabCount = Int(snapshot.dabCount)
        let dripTotal = snapshot.strokes.reduce(0) { $0 + $1.drips.count }
        let bounds = (minU: minU, minV: minV, widthM: widthM, heightM: heightM)
        let sameBounds = bitmapBounds.map {
            $0.minU == minU && $0.minV == minV && $0.widthM == widthM && $0.heightM == heightM
        } ?? false
        // Incremental path: marks only get appended, and only when the same
        // bitmap still covers the painted area.
        let incremental = bitmap != nil && sameBounds
            && dabCount >= renderedFlatCount
            && dripTotal >= renderedDripCount
            && snapshot.strokes.count >= renderedStrokeCount

        if incremental && dabCount == renderedFlatCount && dripTotal == renderedDripCount {
            return // nothing new to draw
        }
        if !incremental {
            guard setupBitmap(widthM: widthM, heightM: heightM) else { return }
            renderedFlatCount = 0
            renderedDripCount = 0
            renderedStrokeCount = 0
        }
        guard let bitmap else { return }
        stampNewPaint(snapshot: snapshot, into: bitmap, bounds: bounds)
        renderedFlatCount = dabCount
        renderedDripCount = dripTotal
        renderedStrokeCount = snapshot.strokes.count
        bitmapBounds = bounds

        guard let image = bitmap.makeImage(),
              let texture = try? TextureResource.generate(
                  from: image,
                  options: .init(semantic: .color)
              )
        else { return }

        var material = UnlitMaterial()
        material.color = .init(tint: .white, texture: .init(texture))
        material.blending = .transparent(opacity: 1.0)

        if incremental, let entity = canvasEntity {
            // Same quad, fresh texture.
            entity.model?.materials = [material]
            return
        }

        // Entity sits on the wall: local X -> canvas u (right), local Y ->
        // canvas normal (out of the wall), local Z -> -v so that +v is "up"
        // in texture space.
        let centerU = (minU + maxU) / 2
        let centerV = (minV + maxV) / 2
        let u = simd(frame.axisU), v = simd(frame.axisV), n = simd(frame.normal)
        let origin = simd(frame.origin) + u * centerU + v * centerV
        var m = matrix_identity_float4x4
        m.columns.0 = SIMD4<Float>(u, 0)
        m.columns.1 = SIMD4<Float>(n, 0)
        m.columns.2 = SIMD4<Float>(-v, 0)
        m.columns.3 = SIMD4<Float>(origin, 1)

        let entity = ModelEntity(mesh: .generatePlane(width: widthM, depth: heightM),
                                 materials: [material])
        entity.setTransformMatrix(m, relativeTo: nil)

        canvasEntity?.parent?.removeFromParent()
        let anchor = AnchorEntity(world: matrix_identity_float4x4)
        anchor.addChild(entity)
        arView.scene.addAnchor(anchor)
        canvasEntity = entity
    }

    func removeCanvas() {
        canvasEntity?.parent?.removeFromParent()
        canvasEntity = nil
        bitmap = nil
        bitmapBounds = nil
        renderedFlatCount = 0
        renderedDripCount = 0
        renderedStrokeCount = 0
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

    // MARK: - Rasterization

    private func setupBitmap(widthM: Float, heightM: Float) -> Bool {
        let pxW = min(Int((CGFloat(widthM) * Self.pixelsPerMeter).rounded(.up)), Self.maxTextureSide)
        let pxH = min(Int((CGFloat(heightM) * Self.pixelsPerMeter).rounded(.up)), Self.maxTextureSide)
        bitmap = CGContext(data: nil, width: pxW, height: pxH,
                           bitsPerComponent: 8, bytesPerRow: 0,
                           space: CGColorSpaceCreateDeviceRGB(),
                           bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        return bitmap != nil
    }

    /// Stamps new marks (dabs with flat index >= `renderedFlatCount`, drips
    /// >= `renderedDripCount`) as gradients. Flat order = stroke order =
    /// append-only under the core's ops.
    private func stampNewPaint(snapshot: CanvasSnapshotF, into ctx: CGContext,
                               bounds: (minU: Float, minV: Float, widthM: Float, heightM: Float)) {
        let pxW = ctx.width, pxH = ctx.height
        let sx = CGFloat(pxW) / CGFloat(bounds.widthM)
        let sy = CGFloat(pxH) / CGFloat(bounds.heightM)
        // Wall-local (u right, v up) -> pixel coords (x right, y down).
        func px(_ u: Float, _ v: Float) -> CGPoint {
            CGPoint(x: CGFloat(u - bounds.minU) * sx,
                    y: CGFloat(bounds.heightM - (v - bounds.minV)) * sy)
        }

        var flatIndex = 0
        var dripIndex = 0
        for stroke in snapshot.strokes {
            let c = stroke.color
            let rgba = (
                CGFloat(c.r) / 255, CGFloat(c.g) / 255,
                CGFloat(c.b) / 255, CGFloat(c.a) / 255
            )
            for dab in stroke.dabs {
                defer { flatIndex += 1 }
                guard flatIndex >= renderedFlatCount else { continue }
                stamp(dab: dab, rgba: rgba, into: ctx, px: px, sx: sx, sy: sy)
            }
            for drip in stroke.drips {
                defer { dripIndex += 1 }
                guard dripIndex >= renderedDripCount else { continue }
                stamp(drip: drip, rgba: rgba, into: ctx, px: px, sx: sx)
            }
        }
    }

    /// One v2 dab: an ellipse (major along the spray tilt) with a radial
    /// gradient whose dense core is offset toward the near side.
    private func stamp(dab: DabF,
                       rgba: (CGFloat, CGFloat, CGFloat, CGFloat),
                       into ctx: CGContext,
                       px: (Float, Float) -> CGPoint,
                       sx: CGFloat, sy: CGFloat) {
        let center = px(dab.u, dab.v)
        let a = CGFloat(dab.radiusMajorM) * sx
        let b = CGFloat(dab.radiusMinorM) * sy
        guard a > 0.3, b > 0.3 else { return }
        let alpha = CGFloat(dab.opacity) * rgba.3
        // Edge falloff: soft marks fade early, crisp ones hold paint to the
        // rim (マーカー vs ふんわり).
        let edge = min(1.0 - 0.65 * CGFloat(dab.softness), 0.98)
        let colors = [
            CGColor(srgbRed: rgba.0, green: rgba.1, blue: rgba.2, alpha: alpha),
            CGColor(srgbRed: rgba.0, green: rgba.1, blue: rgba.2, alpha: alpha * 0.55),
            CGColor(srgbRed: rgba.0, green: rgba.1, blue: rgba.2, alpha: 0),
        ] as CFArray
        guard let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(),
                                        colors: colors,
                                        locations: [0.0, edge, 1.0])
        else { return }
        // Dense-side offset, rotated into ellipse space. Wall +v is pixel -y,
        // so the wall-space rotation `theta` appears as -theta and the dab
        // is drawn in a y-flipped unit space.
        let theta = CGFloat(dab.rotationRad)
        let cosT = cos(theta), sinT = sin(theta)
        let gu = CGFloat(dab.gradientU), gv = CGFloat(dab.gradientV)
        let gx = cosT * gu + sinT * gv
        let gy = -(sinT * gu) + cosT * gv
        ctx.saveGState()
        ctx.translateBy(x: center.x, y: center.y)
        ctx.rotate(by: -theta)
        ctx.scaleBy(x: a, y: b)
        ctx.drawRadialGradient(gradient,
                               startCenter: CGPoint(x: gx * 0.4, y: -gy * 0.4),
                               startRadius: 0,
                               endCenter: .zero, endRadius: 1,
                               options: [.drawsAfterEndLocation])
        ctx.restoreGState()
    }

    /// One drip: a capsule clipped to a top-down gradient — dense at the
    /// head, thinning as the paint runs down (-v).
    private func stamp(drip: DripF,
                       rgba: (CGFloat, CGFloat, CGFloat, CGFloat),
                       into ctx: CGContext,
                       px: (Float, Float) -> CGPoint,
                       sx: CGFloat) {
        let head = px(drip.u, drip.v)
        let tail = px(drip.u, drip.v - drip.lenM)
        let w = max(CGFloat(drip.widthM) * sx, 1.0)
        guard tail.y > head.y else { return }
        let alpha = CGFloat(drip.opacity) * rgba.3
        let colors = [
            CGColor(srgbRed: rgba.0, green: rgba.1, blue: rgba.2, alpha: alpha),
            CGColor(srgbRed: rgba.0, green: rgba.1, blue: rgba.2, alpha: alpha * 0.6),
            CGColor(srgbRed: rgba.0, green: rgba.1, blue: rgba.2, alpha: 0),
        ] as CFArray
        guard let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(),
                                        colors: colors,
                                        locations: [0.0, 0.55, 1.0])
        else { return }
        let path = CGPath(
            roundedRect: CGRect(x: head.x - w / 2, y: head.y,
                                width: w, height: tail.y - head.y),
            cornerWidth: w / 2, cornerHeight: w / 2, transform: nil)
        ctx.saveGState()
        ctx.addPath(path)
        ctx.clip()
        ctx.drawLinearGradient(gradient, start: head, end: tail, options: [])
        ctx.restoreGState()
    }
}

private func simd(_ v: Vec3F) -> SIMD3<Float> {
    SIMD3<Float>(v.x, v.y, v.z)
}
