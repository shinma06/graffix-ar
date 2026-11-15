import SwiftUI

/// Shibuya Graffix AR — AR グラフィティを壁に描くアプリ。
/// すべてのスプレー計算は Rust コア (graffix-core / UniFFI) にあり、
/// このシェルは ARKit の入力を流し込み、返ってきたキャンバスを描画する。
@main
struct GraffixARApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}
