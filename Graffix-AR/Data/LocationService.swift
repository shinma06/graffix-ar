import CoreLocation
import Foundation

/// CoreLocation の薄いラッパー。地図の距離表示・作品保存時の位置取得に使う。
/// 権限が無くてもアプリは動く — 拒否時は `current` が nil のまま。
@MainActor
@Observable
final class LocationService: NSObject {
    private(set) var authorization: CLAuthorizationStatus = .notDetermined
    private(set) var current: CLLocation?

    private let manager = CLLocationManager()
    /// `locationForSave()` が位置を待っている継続。
    private var waiters: [CheckedContinuation<CLLocation?, Never>] = []

    override init() {
        super.init()
        authorization = manager.authorizationStatus
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyNearestTenMeters
        if authorization == .authorizedWhenInUse || authorization == .authorizedAlways {
            manager.startUpdatingLocation()
        }
    }

    /// 最新の測位をコアの `GeoPointF` として返す。未許可・未取得なら nil。
    var geoPoint: GeoPointF? {
        current.map { GeoPointF(lat: $0.coordinate.latitude, lon: $0.coordinate.longitude) }
    }

    /// 権限プロンプトを一度だけ出す。許可済みなら測位を継続開始する。
    func requestAccess() {
        switch manager.authorizationStatus {
        case .notDetermined:
            manager.requestWhenInUseAuthorization()
        case .authorizedWhenInUse, .authorizedAlways:
            manager.startUpdatingLocation()
        default:
            break
        }
    }

    /// 作品保存用のワンショット測位。拒否・失敗時は最後の既知位置か nil。
    func locationForSave() async -> CLLocation? {
        if let current, Date().timeIntervalSince(current.timestamp) < 300 {
            return current
        }
        switch authorization {
        case .authorizedWhenInUse, .authorizedAlways:
            return await withCheckedContinuation { cont in
                waiters.append(cont)
                manager.requestLocation()
            }
        case .notDetermined:
            // プロンプトを出して結果を待つ — 許可なら requestLocation、
            // 拒否なら authorizationDidChange が nil で再開する。
            return await withCheckedContinuation { cont in
                waiters.append(cont)
                manager.requestWhenInUseAuthorization()
            }
        default:
            return current
        }
    }

    private func authorizationDidChange() {
        authorization = manager.authorizationStatus
        switch authorization {
        case .authorizedWhenInUse, .authorizedAlways:
            manager.startUpdatingLocation()
            if !waiters.isEmpty {
                manager.requestLocation()
            }
        case .denied, .restricted:
            flushWaiters(with: nil)
        default:
            break
        }
    }

    private func didUpdate(_ locations: [CLLocation]) {
        guard let latest = locations.last else { return }
        current = latest
        flushWaiters(with: latest)
    }

    private func flushWaiters(with location: CLLocation?) {
        let pending = waiters
        waiters.removeAll()
        for cont in pending {
            cont.resume(returning: location)
        }
    }
}

extension LocationService: CLLocationManagerDelegate {
    nonisolated func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        Task { @MainActor in self.authorizationDidChange() }
    }

    nonisolated func locationManager(
        _ manager: CLLocationManager,
        didUpdateLocations locations: [CLLocation]
    ) {
        Task { @MainActor in self.didUpdate(locations) }
    }

    nonisolated func locationManager(
        _ manager: CLLocationManager,
        didFailWithError error: Error
    ) {
        Task { @MainActor in self.flushWaiters(with: nil) }
    }
}
