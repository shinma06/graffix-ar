# プロジェクト情報

| 項目 | 現在の事実・正本 |
|---|---|
| 目的 | 渋谷の壁に、スプレー缶のようにスマートフォンを動かしてデジタルグラフィティを描く。違法な落書きを合法なデジタル表現に移す |
| Repository | `shinma06/graffix-ar` |
| コンセプト | [`docs/concept/`](concept/)（SHIBUYA+FUN PROJECT 資料と `VISION.md`）。読み取り専用の正本 |
| 実装 | Rust コア `core/`（crate `graffix-core`、UniFFI で公開）+ Swift シェル `Graffix-AR/`（SwiftUI + RealityKit `ARView`）。計算はコア、シェルは入出力と描画のみ。詳細は [README](../README.md) |
| 対応 OS | iOS 27 のみ。AR は LiDAR 搭載の実機が必要（Simulator は ARKit 非対応） |
| ビルド | `scripts/build-xcframework.sh`（XCFramework + UniFFI Swift バインディング）→ Xcode スキーム `Graffix-AR` |
| テスト | `core/` の `cargo test`（単体・統合）。Swift 側の XCTest/UI テストは無い |
| 必須 checks | `.github/workflows/ci.yml` の `Rust core (Linux)` と `iOS app (Xcode)`（macOS・最新安定版 Xcode） |
| 開発 | Devin の親セッションが子セッションへ 1 タスクずつ委任し、レビュー・統合する。セッションは Entire に記録する。進め方は [AGENTS.md](../AGENTS.md)、旧運用は [legacy-ops](legacy-ops/README.md) |
| 統合先 | 子 PR は `rebuild/concept-rust-core` へ。`main` へは PR #7 で所有者が取り込む |
| 受入 | CI はビルドとコアのテストまで。実機での AR（面検出・LiDAR 深度・描画の向き）と触覚は人が確認する |
