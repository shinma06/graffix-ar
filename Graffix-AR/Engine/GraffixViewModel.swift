import Foundation
import SwiftUI
import UIKit
import os

/// UI-facing state. `Engine` (the UniFFI object) is shared with the AR
/// coordinator, which calls `tick` on the AR session queue; everything on
/// this class runs on the main actor.
@Observable
@MainActor
final class GraffixViewModel {
    let engine: Engine
    /// Supported spray colors / 書き方 presets (casual) / ノズル caps (real),
    /// straight from the core. Labels live in `CoreLabels.swift`.
    let sprayColors: [SprayColorValueF]
    let presets: [BrushPresetF]
    let caps: [SprayCapF]

    // Input to the engine, set by the spray button and read on the AR queue.
    let sprayInput = SprayInput()

    // Core-driven haptics and the 缶振り gesture detector.
    let haptics = SprayHaptics()
    private let shakeDetector = ShakeDetector()

    // Renderer is attached once the ARView exists.
    private(set) var renderer: WallCanvasRenderer?

    // MARK: - UI state

    private(set) var aimedWallID: UInt64?
    private(set) var lockedWallID: UInt64?
    private(set) var wallCount: UInt32 = 0
    private(set) var distanceM: Float?
    private(set) var distanceReliability: Float = 0
    private(set) var lidarFused = false
    private(set) var strokeActive = false
    private(set) var canUndo = false
    private(set) var canRedo = false
    /// Can-shake level 0..1 reported by the core (real mode's かすれ meter).
    private(set) var shakeLevel: Float = 0

    var sprayMode: SprayModeF = .casual {
        didSet {
            engine.setMode(mode: sprayMode)
            haptics.click()
        }
    }

    var preset: BrushPresetF = .broad {
        didSet {
            engine.setPreset(preset: preset)
            haptics.click()
        }
    }

    var cap: SprayCapF = .standard {
        didSet {
            engine.setCap(cap: cap)
            // キャップ交換のクリック感。
            haptics.click()
        }
    }

    var color: SprayColorF = .white {
        didSet { engine.setColor(color: color) }
    }

    var spraying = false {
        didSet { sprayInput.set(spraying) }
    }

    init(engine: Engine) {
        self.engine = engine
        sprayColors = supportedSprayColors()
        presets = supportedBrushPresets()
        caps = supportedSprayCaps()
        engine.setColor(color: color)
        engine.setMode(mode: sprayMode)
        engine.setPreset(preset: preset)
        engine.setCap(cap: cap)
        shakeDetector.onShake = { [weak self] in
            Task { @MainActor in self?.canShaken() }
        }
    }

    func attach(renderer: WallCanvasRenderer) {
        self.renderer = renderer
    }

    // MARK: - Status text

    var statusText: String {
        if lockedWallID != nil {
            return strokeActive ? "描画中…" : "壁にロック中 — スプレーを押して描く"
        }
        if aimedWallID != nil {
            return "壁を検出 — 「ロック」で描画面を決定"
        }
        return wallCount > 0 ? "壁にレティクルを合わせてください" : "壁を探しています…"
    }

    var distanceText: String? {
        guard let d = distanceM else { return nil }
        return String(format: "距離 %.1fm", d)
    }

    var canLock: Bool { aimedWallID != nil && lockedWallID == nil }
    var isLocked: Bool { lockedWallID != nil }

    // MARK: - Frame application (called on main actor from the coordinator)

    func apply(frameOutput out: FrameOutputF) {
        aimedWallID = out.aimedWallId
        lockedWallID = out.lockedWallId
        distanceM = out.distanceM
        distanceReliability = out.distanceReliability
        lidarFused = out.distanceLidarFused
        strokeActive = out.strokeActive
        shakeLevel = out.spray.shakeLevel
        wallCount = engine.wallCount()
        canUndo = engine.canUndo()
        canRedo = engine.canRedo()
        // Haptics run from core truth: on/off, flow, density.
        haptics.updateSpray(out.spray)

        if out.canvasDirty {
            renderCanvas()
        }
    }

    private func renderCanvas() {
        renderer?.requestCanvasRender(engine: engine)
    }

    // MARK: - User actions

    func lockAimed() {
        _ = engine.lockAimed()
    }

    func unlock() {
        engine.unlock()
    }

    func undo() {
        _ = engine.undo()
        renderCanvas()
        canUndo = engine.canUndo()
        canRedo = engine.canRedo()
    }

    func redo() {
        _ = engine.redo()
        renderCanvas()
        canUndo = engine.canUndo()
        canRedo = engine.canRedo()
    }

    func clearCanvas() {
        engine.clear()
        renderCanvas()
        canUndo = engine.canUndo()
        canRedo = engine.canRedo()
    }

    /// ARView のスナップショット（作品撮影→共有用）。
    func takeSnapshot() async -> UIImage? {
        await renderer?.snapshot()
    }

    // MARK: - 缶を振る (shake detection)

    func startShakeDetection() {
        shakeDetector.start()
    }

    func stopShakeDetection() {
        shakeDetector.stop()
    }

    /// Device-shake gesture: rattle the can in the core and the hands.
    private func canShaken() {
        engine.shake()
        haptics.rattle()
    }
}

/// Spray-button state shared between the main actor (UI) and the AR delegate
/// queue (`Engine::tick` input). `Engine` is `Sendable`; this is the only UI
/// bit that crosses threads.
final class SprayInput: Sendable {
    private let flag = OSAllocatedUnfairLock(initialState: false)

    func set(_ value: Bool) {
        flag.withLock { $0 = value }
    }

    var value: Bool {
        flag.withLock { $0 }
    }
}
