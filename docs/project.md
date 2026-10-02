# プロジェクト情報

| 項目 | 事実・正本 |
|---|---|
| 目的 | カメラ映像から壁面を検出し、壁までの距離を測定する iOS アプリ |
| Repository | `shinma06/graffix-ar` |
| 実装 | Swift 5 モード、SwiftUI、ARKit、SceneKit。View / ViewModel / Service / Model の構成 |
| 対応 OS | `IPHONEOS_DEPLOYMENT_TARGET = 18.1`。実機で AR 対応とカメラ許可が必要 |
| Project / scheme | `Graffix-AR.xcodeproj` / 共有 scheme `Mock Up` |
| Entry | `Graffix-AR/App/Graffix-ARApp.swift` → `Features/WallDetection/Views/ContentView.swift` |
| 主な機能 | `Features/WallDetection`、`Features/DistanceMeasurement`、共通 AR 構成は `Features/Common` |
| 共通処理 | `Common` のエラー処理・ViewModel・拡張、`Resources/Localization` の日本語表示 |
| Harness | Python 3.11 以上、標準ライブラリ、Bash。macOS/Linux（iOS build は macOS のみ） |
| 統合先 | 実装は既存 `develop`。検証済み候補と GUI 不要の運用変更は `main`。既存 branch を作り直さない |
| 必須 checks | `test` / `PR policy` / `Acceptance gate` / `Agent review`。サーバー適用状況は [検証記録](validation.md) |
| 作業管理 | [Issue / Project / Milestone / Relationship](work-management.md) |

## 実行する確認

```bash
# 管理ファイル・リンク・Python 回帰テスト。新規の未追跡ファイルも対象
python3 scripts/check.py
# Xcode の共有 scheme を署名なしで Simulator 向けにビルド
python3 scripts/workflow/ios_build.py
# commit 済みの変更に応じた確認。base は実際の PR base に合わせる
python3 scripts/workflow/change_impact.py --base origin/develop --run-tests
```

`ios_build.py` は `xcodebuild -project Graffix-AR.xcodeproj -scheme 'Mock Up'` を使用します。Xcode の選択はマシン側の `DEVELOPER_DIR` または `xcode-select` を使い、個人のインストールパスを共有設定に固定しません。外部依存の追加はありません。

現状、Xcode のテスト target はありますがテストソースと scheme の Testables はありません。`xcodebuild test` を成功済みと扱わず、ハーネス回帰テストとアプリ build を区別します。XCTest を追加したときは `ios_build.py`・CI・変更影響判定へ実 test コマンドを接続します。

壁面検出、距離精度、ARSession の中断・復帰、カメラ許可拒否、発熱・長時間使用は [実機受入](verification/README.md) で確認します。Simulator のコンパイルはこれらを代替しません。Swift の非同期処理、UI 更新、ARSession/Delegate の所有と解放、入力・エラー処理を変更範囲に応じて確認します。Android の ViewBinding・Gradle・Lifecycle 規約はこのプロジェクトには適用しません。

`repomix-output.txt` は初期の集約スナップショットです。現行コード・規約・検証結果の正本には使用せず、今回の導入でも再生成していません。
