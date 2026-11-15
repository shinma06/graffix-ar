import CoreLocation
import Foundation

/// ローカルに保存した自分の作品 1 件。
/// ファイルは Documents/works/<id>.json（本レコード）と <id>.gfxc
/// （GFXC シリアライズ済みキャンバス）に分かれる。
/// 本格的な作品管理ページは後のパス。ここではマップのピンと簡易一覧に
/// 必要な分だけ保持する。
struct LocalWorkRecord: Codable, Identifiable, Hashable {
    /// UUID。ファイル名（<id>.json / <id>.gfxc）にも使う。
    var id: String
    var title: String
    /// Unix epoch 秒（GFXC の created と同じ基準）。
    var createdAt: TimeInterval
    /// 保存時の位置。権限が無い・未取得なら nil で、ピンは出ない。
    var latitude: Double?
    var longitude: Double?

    var coordinate: CLLocationCoordinate2D? {
        guard let latitude, let longitude else { return nil }
        return CLLocationCoordinate2D(latitude: latitude, longitude: longitude)
    }
}

/// Documents ディレクトリ内の簡易作品ストア。
/// 位置付きの保存を `.workMine` ピンとしてマップに載せる。
@MainActor
@Observable
final class LocalWorkStore {
    private(set) var works: [LocalWorkRecord] = []

    private let worksDir: URL
    private let encoder = JSONEncoder()
    private let decoder = JSONDecoder()

    init(fileManager: FileManager = .default) {
        let docs = fileManager.urls(for: .documentDirectory, in: .userDomainMask)[0]
        worksDir = docs.appendingPathComponent("works", isDirectory: true)
        try? fileManager.createDirectory(at: worksDir, withIntermediateDirectories: true)
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        reload()
    }

    func reload() {
        let urls = (try? FileManager.default.contentsOfDirectory(
            at: worksDir,
            includingPropertiesForKeys: nil
        )) ?? []
        works = urls
            .filter { $0.pathExtension == "json" }
            .compactMap { url in
                (try? Data(contentsOf: url)).flatMap { try? decoder.decode(LocalWorkRecord.self, from: $0) }
            }
            .sorted { $0.createdAt > $1.createdAt }
    }

    /// キャンバスを GFXC バイト列として保存し、レコードを返す。
    @discardableResult
    func save(
        canvasBytes: Data,
        title: String,
        coordinate: CLLocationCoordinate2D?
    ) throws -> LocalWorkRecord {
        let id = UUID().uuidString
        let record = LocalWorkRecord(
            id: id,
            title: title,
            createdAt: Date().timeIntervalSince1970,
            latitude: coordinate?.latitude,
            longitude: coordinate?.longitude
        )
        try canvasBytes.write(to: worksDir.appendingPathComponent("\(id).gfxc"))
        try encoder.encode(record).write(to: recordURL(id: id))
        works.insert(record, at: 0)
        return record
    }

    func delete(_ record: LocalWorkRecord) {
        try? FileManager.default.removeItem(at: recordURL(id: record.id))
        try? FileManager.default.removeItem(
            at: worksDir.appendingPathComponent("\(record.id).gfxc"))
        works.removeAll { $0.id == record.id }
    }

    /// `.workMine` ピンに変換。位置を持たない作品は nil。
    func mapSpot(for record: LocalWorkRecord) -> MapSpotF? {
        guard let coordinate = record.coordinate else { return nil }
        let date = Date(timeIntervalSince1970: record.createdAt)
        return MapSpotF(
            id: record.id,
            kind: .workMine,
            position: GeoPointF(lat: coordinate.latitude, lon: coordinate.longitude),
            title: record.title,
            subtitle: date.formatted(.dateTime.year().month().day().hour().minute()),
            authorUrl: nil
        )
    }

    /// 保存時のデフォルトタイトル。
    static func defaultTitle(now: Date = Date()) -> String {
        "作品 " + now.formatted(.dateTime.month().day().hour().minute())
    }

    private func recordURL(id: String) -> URL {
        worksDir.appendingPathComponent("\(id).json")
    }
}
