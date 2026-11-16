import CoreHaptics
import Foundation

/// Core Haptics driver fed entirely by the core's `SprayStateF` — the shell
/// plays what the engine reports, it never invents spray state.
///
/// - While `spraying`: a continuous pattern runs, intensity = normalized
///   paint flow, sharpness = per-dab density (so a fat cap up close feels
///   different from a needle far away).
/// - `rattle()`: the mixing-ball rattle when the can is shaken.
/// - `click()`: cap/preset change click.
///
/// Everything degrades silently on devices without haptic support and on
/// engine failure — haptics are feedback, never a failure path.
@MainActor
final class SprayHaptics {
    private let supported = CHHapticEngine.capabilitiesForHardware().supportsHaptics
    private var engine: CHHapticEngine?
    private var sprayPlayer: CHHapticAdvancedPatternPlayer?
    private var spraying = false

    init() {
        prepare()
    }

    private func prepare() {
        guard supported, engine == nil else { return }
        engine = try? CHHapticEngine()
        guard let engine else { return }
        engine.isAutoShutdownEnabled = true
        engine.resetHandler = { [weak self] in
            _ = try? self?.engine?.start()
        }
        try? engine.start()
    }

    // MARK: - Continuous spray

    /// Called once per applied frame with the core's spray truth.
    func updateSpray(_ state: SprayStateF) {
        guard supported else { return }
        if state.spraying, !spraying {
            startSpray(state)
        } else if state.spraying {
            modulate(state)
        } else if spraying {
            stopSpray()
        }
        spraying = state.spraying
    }

    private func startSpray(_ state: SprayStateF) {
        guard let engine else { return }
        try? engine.start()
        let event = CHHapticEvent(
            eventType: .hapticContinuous,
            parameters: [
                CHHapticEventParameter(parameterID: .hapticIntensity,
                                       value: max(state.flow, 0.05)),
                CHHapticEventParameter(parameterID: .hapticSharpness,
                                       value: state.density),
            ],
            relativeTime: 0,
            duration: 60 * 60)
        guard let pattern = try? CHHapticPattern(events: [event], parameters: []),
              let player = try? engine.makeAdvancedPlayer(with: pattern)
        else { return }
        sprayPlayer = player
        try? player.start(atTime: CHHapticTimeImmediate)
    }

    private func modulate(_ state: SprayStateF) {
        guard let sprayPlayer else { return }
        try? sprayPlayer.sendParameters(
            [
                CHHapticDynamicParameter(parameterID: .hapticIntensityControl,
                                         value: max(state.flow, 0.05),
                                         relativeTime: 0),
                CHHapticDynamicParameter(parameterID: .hapticSharpnessControl,
                                         value: state.density,
                                         relativeTime: 0),
            ],
            atTime: CHHapticTimeImmediate)
    }

    private func stopSpray() {
        try? sprayPlayer?.stop(atTime: CHHapticTimeImmediate)
        sprayPlayer = nil
    }

    // MARK: - Discrete events

    /// カラカラ: the mixing ball rattling inside the can — a burst of sharp
    /// transients like the ball hitting the can walls.
    func rattle() {
        guard supported, let engine else { return }
        try? engine.start()
        var events: [CHHapticEvent] = []
        for i in 0..<6 {
            events.append(CHHapticEvent(
                eventType: .hapticTransient,
                parameters: [
                    CHHapticEventParameter(parameterID: .hapticIntensity,
                                           value: i % 2 == 0 ? 0.8 : 0.55),
                    CHHapticEventParameter(parameterID: .hapticSharpness, value: 0.9),
                ],
                relativeTime: TimeInterval(i) * 0.085))
        }
        guard let pattern = try? CHHapticPattern(events: events, parameters: []),
              let player = try? engine.makePlayer(with: pattern)
        else { return }
        try? player.start(atTime: CHHapticTimeImmediate)
    }

    /// Cap/preset swap: one crisp mechanical click.
    func click() {
        guard supported, let engine else { return }
        try? engine.start()
        let event = CHHapticEvent(
            eventType: .hapticTransient,
            parameters: [
                CHHapticEventParameter(parameterID: .hapticIntensity, value: 0.9),
                CHHapticEventParameter(parameterID: .hapticSharpness, value: 1.0),
            ],
            relativeTime: 0)
        guard let pattern = try? CHHapticPattern(events: [event], parameters: []),
              let player = try? engine.makePlayer(with: pattern)
        else { return }
        try? player.start(atTime: CHHapticTimeImmediate)
    }
}
