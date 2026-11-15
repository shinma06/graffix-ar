import CoreMotion
import Foundation

/// 缶を振る検出: CoreMotion accelerometer shake detection. Fires `onShake`
/// on the main queue, debounced, when a sharp shake is seen. Used to drive
/// `Engine#shake()` (can mixing) — the gesture, not a UI button, because the
/// physical motion is the point.
///
/// `@unchecked Sendable`: `lastFire` is only touched on the serial
/// accelerometer queue, `onShake` is set once before `start()`.
final class ShakeDetector: @unchecked Sendable {
    /// Acceleration magnitude (in g) that counts as a shake stroke.
    private let threshold: Double = 1.9
    /// Minimum time between reported shakes.
    private let debounce: TimeInterval = 0.35

    private let manager = CMMotionManager()
    private let queue = OperationQueue()
    private var lastFire = Date.distantPast

    var onShake: (() -> Void)?

    func start() {
        guard manager.isAccelerometerAvailable, !manager.isAccelerometerActive else { return }
        manager.accelerometerUpdateInterval = 1.0 / 60.0
        manager.startAccelerometerUpdates(to: queue) { [weak self] data, _ in
            guard let self, let data else { return }
            let a = data.acceleration
            let magnitude = (a.x * a.x + a.y * a.y + a.z * a.z).squareRoot()
            guard magnitude > self.threshold else { return }
            let now = Date()
            guard now.timeIntervalSince(self.lastFire) > self.debounce else { return }
            self.lastFire = now
            DispatchQueue.main.async {
                self.onShake?()
            }
        }
    }

    func stop() {
        manager.stopAccelerometerUpdates()
    }
}
