import Foundation

/// UI テスト用の決定的な保存作品。GFXC（`core/src/serialize.rs` の
/// ワイヤ形式）をテスト側で組み立て、起動環境変数でアプリに渡す。
/// アプリ側は Debug ビルドで `-GraffixUITestSeedWorks` があるときだけ
/// これを専用ディレクトリに書き出す（`LocalWorkStore.swift`）。
enum WorkFixtures {
    static let launchArgument = "-GraffixUITestSeedWorks"
    static let environmentKey = "GRAFFIX_UITEST_WORKS"

    struct Fixture {
        let id: String
        let title: String
        let createdAt: TimeInterval
        let latitude: Double?
        let longitude: Double?
        let canvas: Data
    }

    /// v3: 3 ストローク、記録時刻つき（300 ms/ダブ、ストローク間 300 ms）。
    static let timedV3 = Fixture(
        id: "UITEST-V3-TIMED",
        title: "UIテスト v3 時刻あり",
        createdAt: 1_794_700_000,
        latitude: 35.6595,
        longitude: 139.7005,
        canvas: GFXC.encode(version: 3, created: 1_794_700_000, strokes: timedStrokes)
    )

    /// v2: 2 ストローク、時刻なし（旧形式）。
    static let legacyV2 = Fixture(
        id: "UITEST-V2-LEGACY",
        title: "UIテスト v2 旧形式",
        createdAt: 1_731_600_000,
        latitude: nil,
        longitude: nil,
        canvas: GFXC.encode(version: 2, created: 1_731_600_000, strokes: legacyStrokes)
    )

    static let all = [timedV3, legacyV2]

    /// アプリの `UITestWorkFixtures` が読む JSON（`[{record, canvas}]`、canvas は base64）。
    static func environmentPayload(_ fixtures: [Fixture] = all) -> String {
        struct Record: Encodable {
            let id: String
            let title: String
            let createdAt: TimeInterval
            let latitude: Double?
            let longitude: Double?
        }
        struct Entry: Encodable {
            let record: Record
            let canvas: Data
        }
        let entries = fixtures.map {
            Entry(record: Record(id: $0.id, title: $0.title, createdAt: $0.createdAt,
                                 latitude: $0.latitude, longitude: $0.longitude),
                  canvas: $0.canvas)
        }
        let data = try! JSONEncoder().encode(entries)
        return String(decoding: data, as: UTF8.self)
    }

    // MARK: - Strokes

    private static let dabsPerStroke = 61
    /// `MAX_PAUSE_MS`（400 ms）未満なので圧縮されず、全体は約 54.6 秒。
    /// 遅い CI ランナーでも ×1 再生の途中で一時停止できる長さにしている。
    private static let dabIntervalMs: UInt32 = 300
    private static let strokeGapMs: UInt32 = 300

    private static var timedStrokes: [GFXC.Stroke] {
        let n = dabsPerStroke
        let line = (0..<n).map { i -> GFXC.Dab in
            let t = Float(i) / Float(n - 1)
            return .circle(u: -0.6 + 1.2 * t, v: 0.45, radius: 0.05, opacity: 0.9, softness: 0.4)
        }
        let diagonal = (0..<n).map { i -> GFXC.Dab in
            let t = Float(i) / Float(n - 1)
            return .circle(u: -0.6 + 1.2 * t, v: 0.0 + 0.6 * t, radius: 0.04, opacity: 0.85, softness: 0.3)
        }
        let ring = (0..<n).map { i -> GFXC.Dab in
            let a = 2 * Float.pi * Float(i) / Float(n - 1)
            return GFXC.Dab(u: 0.25 * cos(a), v: 0.3 + 0.25 * sin(a),
                            radiusMajor: 0.06, radiusMinor: 0.03, rotation: a,
                            gradientU: cos(a), gradientV: sin(a), opacity: 0.8, softness: 0.5)
        }
        let strokeMs = UInt32(n - 1) * dabIntervalMs
        let offsets = (0..<n).map { UInt32($0) * dabIntervalMs }
        return [line, diagonal, ring].enumerated().map { k, dabs in
            let start = UInt32(k) * (strokeMs + strokeGapMs)
            let drips: [GFXC.Drip] = k == 1
                ? [GFXC.Drip(u: 0.6, v: 0.6, len: 0.2, width: 0.01, opacity: 0.8)] : []
            return GFXC.Stroke(
                paletteIndex: [3, 8, 5][k],
                dabs: dabs,
                drips: drips,
                timing: GFXC.Timing(startMs: start, dabMs: offsets,
                                    dripMs: drips.map { _ in strokeMs })
            )
        }
    }

    private static var legacyStrokes: [GFXC.Stroke] {
        let n = 40
        let a = (0..<n).map { i -> GFXC.Dab in
            let t = Float(i) / Float(n - 1)
            return .circle(u: -0.4 + 0.8 * t, v: 0.2, radius: 0.06, opacity: 0.9, softness: 0.5)
        }
        let b = (0..<n).map { i -> GFXC.Dab in
            let t = Float(i) / Float(n - 1)
            return .circle(u: 0.0, v: -0.1 + 0.6 * t, radius: 0.05, opacity: 0.9, softness: 0.5)
        }
        return [
            GFXC.Stroke(paletteIndex: 6, dabs: a,
                        drips: [GFXC.Drip(u: 0.4, v: 0.2, len: 0.15, width: 0.008, opacity: 0.8)],
                        timing: nil),
            GFXC.Stroke(paletteIndex: 10, dabs: b, drips: [], timing: nil),
        ]
    }
}

/// GFXC v2/v3 エンコーダ（テスト専用。形式は `core/src/serialize.rs` 参照）。
enum GFXC {
    struct Dab {
        var u, v, radiusMajor, radiusMinor, rotation, gradientU, gradientV, opacity, softness: Float

        static func circle(u: Float, v: Float, radius: Float, opacity: Float, softness: Float) -> Dab {
            Dab(u: u, v: v, radiusMajor: radius, radiusMinor: radius, rotation: 0,
                gradientU: 0, gradientV: 0, opacity: opacity, softness: softness)
        }
    }

    struct Drip {
        var u, v, len, width, opacity: Float
    }

    struct Timing {
        var startMs: UInt32
        var dabMs: [UInt32]
        var dripMs: [UInt32]
    }

    struct Stroke {
        var paletteIndex: UInt8
        var dabs: [Dab]
        var drips: [Drip]
        /// v3 のみ。v2 では書き出さない。
        var timing: Timing?
    }

    static func encode(version: UInt16, created: UInt64, strokes: [Stroke]) -> Data {
        precondition(version == 2 || version == 3)
        var out = [UInt8]()
        func u8(_ x: UInt8) { out.append(x) }
        func u16(_ x: UInt16) { withUnsafeBytes(of: x.littleEndian) { out.append(contentsOf: $0) } }
        func u32(_ x: UInt32) { withUnsafeBytes(of: x.littleEndian) { out.append(contentsOf: $0) } }
        func u64(_ x: UInt64) { withUnsafeBytes(of: x.littleEndian) { out.append(contentsOf: $0) } }
        func f32(_ x: Float) { u32(x.bitPattern) }

        u32(0x4746_5843) // "GFXC"
        u16(version)
        u16(20) // header_len
        u64(created)
        u8(0) // has_frame
        out.append(contentsOf: [0, 0, 0])
        u32(UInt32(strokes.count))
        for s in strokes {
            let timing = version >= 3 ? s.timing : nil
            u8(s.paletteIndex)
            u8(timing == nil ? 0 : 1)
            out.append(contentsOf: [0, 0])
            u32(UInt32(s.dabs.count))
            for d in s.dabs {
                for f in [d.u, d.v, d.radiusMajor, d.radiusMinor, d.rotation,
                          d.gradientU, d.gradientV, d.opacity, d.softness] { f32(f) }
            }
            u32(UInt32(s.drips.count))
            for d in s.drips {
                for f in [d.u, d.v, d.len, d.width, d.opacity] { f32(f) }
            }
            if let timing {
                precondition(timing.dabMs.count == s.dabs.count && timing.dripMs.count == s.drips.count)
                u32(timing.startMs)
                timing.dabMs.forEach(u32)
                timing.dripMs.forEach(u32)
            }
        }
        u32(crc32(out))
        return Data(out)
    }

    private static func crc32(_ bytes: [UInt8]) -> UInt32 {
        var crc: UInt32 = 0xFFFF_FFFF
        for b in bytes {
            crc ^= UInt32(b)
            for _ in 0..<8 {
                crc = (crc & 1) != 0 ? (crc >> 1) ^ 0xEDB8_8320 : crc >> 1
            }
        }
        return ~crc
    }
}
