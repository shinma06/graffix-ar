# プロジェクト情報

| 項目 | 現在の事実・正本 |
|---|---|
| 目的 | 渋谷の壁に、スプレー缶のようにスマートフォンを動かしてデジタルグラフィティを描く。違法な落書きを合法なデジタル表現に移す |
| Repository | `shinma06/graffix-ar` |
| コンセプト | [`docs/concept/`](concept/)（SHIBUYA+FUN PROJECT 資料と `VISION.md`）。読み取り専用の正本 |
| 実装 | Rust コア `core/`（crate `graffix-core`、UniFFI で公開）+ Swift シェル `Graffix-AR/`（SwiftUI + RealityKit `ARView`）。計算と状態はコア、表示文言・並び・操作はシェル（コアは安定 ID・値・範囲のみ返す）。塗りのラスタライズ（AR の壁テクスチャ、保存作品のプレビュー・タイムラプス・動画書き出し）はコアの `raster.rs` 1 実装で、シェルは返った画素を表示・転送するだけ。詳細は [README](../README.md) |
| 対応 OS | iOS 27 のみ。AR は LiDAR 搭載の実機が必要（Simulator は ARKit 非対応） |
| ビルド | `scripts/build-xcframework.sh`（XCFramework + UniFFI Swift バインディング）→ Xcode スキーム `Graffix-AR` |
| テスト | `core/` の `cargo test`（単体・統合）と、作品詳細の UI テスト `Graffix-ARUITests`（XCUITest、iOS 27 Simulator でシードした保存作品を操作）。それ以外の Swift 側テストは無い |
| 必須 checks | `.github/workflows/ci.yml` の `Rust core (Linux)` と `iOS app (Xcode)`（macOS・最新安定版 Xcode） |
| 開発 | Devin の親セッションが子セッションへ 1 タスクずつ委任し、レビュー・統合する。セッションは Entire に記録する。進め方は [AGENTS.md](../AGENTS.md)、旧運用は [legacy-ops](legacy-ops/README.md) |
| 統合先 | 子 PR は `main` へ。親セッションがレビューし、必須 checks 2 件が緑のものを `main` にマージする |
| 受入 | CI はビルド・コアのテスト・Simulator での作品詳細 UI テストまで。実機での AR（面検出・LiDAR 深度・描画の向き）と触覚は人が確認する |
