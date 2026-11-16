import CoreLocation
import Foundation

extension MapSpotF: Identifiable {}

extension MapSpotF {
    var coordinate: CLLocationCoordinate2D {
        CLLocationCoordinate2D(latitude: position.lat, longitude: position.lon)
    }
}

extension GeoPointF {
    init(_ coordinate: CLLocationCoordinate2D) {
        self.init(lat: coordinate.latitude, lon: coordinate.longitude)
    }
}

/// `spots.json`（同梱シード）のデコード用 DTO。
/// フィールド名は JSON 側の snake/lower camel に合わせる。
private struct SpotsFile: Decodable {
    struct Seed: Decodable {
        var id: String
        var title: String
        var subtitle: String?
        var latitude: Double
        var longitude: Double
    }

    var formatVersion: Int
    var notice: String
    var spots: [Seed]
}

/// 同梱 `Resources/spots.json` から公認ウォールのシードを読む。
/// ※ 同梱データはすべてサンプル — 実在の公認ウォールではない。
enum LegalWallSeed {
    static func load() -> [MapSpotF] {
        guard let url = Bundle.main.url(forResource: "spots", withExtension: "json"),
              let data = try? Data(contentsOf: url),
              let file = try? JSONDecoder().decode(SpotsFile.self, from: data)
        else {
            return []
        }
        return file.spots.map { seed in
            MapSpotF(
                id: seed.id,
                kind: .legalWall,
                position: GeoPointF(lat: seed.latitude, lon: seed.longitude),
                title: seed.title,
                subtitle: seed.subtitle,
                authorUrl: nil
            )
        }
    }
}

/// マップ画面の状態。スポットの収集・絞り込み・並び順はすべてコア
/// （`visibleMapSpots` / `haversineM`）に委ねる。
@MainActor
@Observable
final class MapViewModel {
    private(set) var legalWalls: [MapSpotF] = []
    private(set) var otherWorkPins: [MapSpotF] = []
    private(set) var loaded = false

    private let worksStore: LocalWorkStore
    private let location: LocationService
    private let workPinProvider: WorkPinProvider

    init(
        worksStore: LocalWorkStore,
        location: LocationService,
        workPinProvider: WorkPinProvider
    ) {
        self.worksStore = worksStore
        self.location = location
        self.workPinProvider = workPinProvider
    }

    /// 全ピン。自分の作品はストアから都度変換するので保存・削除が即反映される。
    var spots: [MapSpotF] {
        legalWalls + otherWorkPins + worksStore.works.compactMap { worksStore.mapSpot(for: $0) }
    }

    /// 近い順 — 一覧表示用。位置未取得でも入力順で全件返る（コア側の挙動）。
    var sortedSpots: [MapSpotF] {
        visibleMapSpots(
            spots: spots,
            kinds: [.legalWall, .workMine, .workOther],
            center: location.geoPoint,
            radiusM: nil
        )
    }

    /// 初回表示時にシード・他人作品ピン・位置権限をまとめて起動する。
    func load() async {
        guard !loaded else { return }
        legalWalls = LegalWallSeed.load()
        location.requestAccess()
        otherWorkPins = await workPinProvider.workPins(near: location.geoPoint)
        loaded = true
    }

    /// 現在地からの距離ラベル（"350 m" / "1.2 km"）。位置未取得なら nil。
    func distanceText(to spot: MapSpotF) -> String? {
        guard let here = location.geoPoint else { return nil }
        let meters = haversineM(a: here, b: spot.position)
        return meters < 1000
            ? String(format: "%.0f m", meters)
            : String(format: "%.1f km", meters / 1000)
    }

    /// 全スポットの外接矩形 + 余白。データが空なら nil（呼び出し側で .automatic）。
    var boundingRegion: (center: CLLocationCoordinate2D, latDelta: Double, lonDelta: Double)? {
        let all = spots
        guard !all.isEmpty else { return nil }
        let lats = all.map(\.position.lat)
        let lons = all.map(\.position.lon)
        let minLat = lats.min()!, maxLat = lats.max()!
        let minLon = lons.min()!, maxLon = lons.max()!
        return (
            CLLocationCoordinate2D(
                latitude: (minLat + maxLat) / 2,
                longitude: (minLon + maxLon) / 2
            ),
            max((maxLat - minLat) * 1.4, 0.01),
            max((maxLon - minLon) * 1.4, 0.01)
        )
    }
}
