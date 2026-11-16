import Foundation

// 表示文言はシェルの責務。コアは安定 ID と値だけを返す。

extension BrushPresetF {
    /// 書き方（カジュアル）の表示名。
    var label: String {
        switch self {
        case .broad: "太ブラシ"
        case .finePen: "細ペン"
        case .marker: "マーカー"
        case .soft: "ふんわり"
        }
    }
}

extension SprayCapF {
    /// ノズル（リアル）の表示名。
    var label: String {
        switch self {
        case .fat: "ファット"
        case .standard: "スタンダード"
        case .skinny: "スキニー"
        case .needle: "ニードル"
        }
    }
}

extension SprayColorF {
    /// スプレー色の表示名。
    var label: String {
        switch self {
        case .white: "ホワイト"
        case .black: "ブラック"
        case .silver: "シルバー"
        case .red: "レッド"
        case .orange: "オレンジ"
        case .yellow: "イエロー"
        case .green: "グリーン"
        case .teal: "ティール"
        case .blue: "ブルー"
        case .purple: "パープル"
        case .pink: "ピンク"
        case .brown: "ブラウン"
        }
    }
}

extension SceneLightingF {
    var label: String {
        switch self {
        case .day: "昼（晴れ）"
        case .overcast: "昼（曇り）"
        case .dusk: "夕方"
        case .night: "夜（街灯）"
        }
    }
}

extension WallMaterialF {
    var label: String {
        switch self {
        case .concrete: "コンクリート"
        case .shutter: "シャッター"
        case .brick: "レンガ"
        case .plaster: "モルタル（白壁）"
        }
    }
}

extension WallColorF {
    var label: String {
        switch self {
        case .white: "ホワイト"
        case .gray: "グレー"
        case .black: "ブラック"
        case .beige: "ベージュ"
        case .navy: "ネイビー"
        case .mint: "ミント"
        }
    }
}
