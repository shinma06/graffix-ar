import Foundation

/// 他人の作品ピンの供給元。
/// フェーズ2の作品インデックスはこのプロトコルの実装を差し替えるだけで
/// よく、マップ画面側のコードは変わらない。
protocol WorkPinProvider: Sendable {
    /// `center` 周辺の他人の作品ピン。`center == nil` は位置未取得を意味し、
    /// その場合の挙動（全件・空等）は実装側の自由。
    func workPins(near center: GeoPointF?) async -> [MapSpotF]
}

/// フェーズ2のインデックスが来るまでの仮実装。
/// 返すピンはすべてサンプル（作者リンクも example.com のプレースホルダ）。
struct MockWorkPinProvider: WorkPinProvider {
    func workPins(near center: GeoPointF?) async -> [MapSpotF] {
        MockWorkPinProvider.samplePins
    }

    static let samplePins: [MapSpotF] = [
        MapSpotF(
            id: "sample-work-01",
            kind: .workOther,
            position: GeoPointF(lat: 35.6602, lon: 139.7022),
            title: "サンプル作品「FLOW」",
            subtitle: "サンプル作者 @sample_tokyo_writer",
            authorUrl: "https://example.com/sample_tokyo_writer"
        ),
        MapSpotF(
            id: "sample-work-02",
            kind: .workOther,
            position: GeoPointF(lat: 35.6563, lon: 139.6998),
            title: "サンプル作品「夜霧」",
            subtitle: "サンプル作者 @midnight_can",
            authorUrl: "https://example.com/midnight_can"
        ),
        MapSpotF(
            id: "sample-work-03",
            kind: .workOther,
            position: GeoPointF(lat: 35.6591, lon: 139.7051),
            title: "サンプル作品「BUFF ME NOT」",
            subtitle: "サンプル作者 @wallrunner",
            authorUrl: "https://example.com/wallrunner"
        ),
    ]
}
