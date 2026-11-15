# Shibuya Graffix AR

渋谷の壁に、スプレー缶のようにスマートフォンを動かしてデジタル
グラフィティを描く iOS アプリです。違法な落書きで街の景観を
損なうのではなく、HIPHOP 文化であるグラフィティをポジティブで
合法なデジタル表現に移します。

コンセプト: [`docs/concept/SHIBUYA-FUN-PROJECT.pdf`](docs/concept/SHIBUYA-FUN-PROJECT.pdf)、
追加ビジョン（フェーズ 2/3）: [`docs/concept/VISION.md`](docs/concept/VISION.md)。
フェーズ 2 設計メモ（提案・オーナー判断待ち）: [`docs/design/phase2.md`](docs/design/phase2.md)。

**フェーズ 1（本実装）** — ローカルの描画体験:
壁の認識・ロック → 距離連動スプレー描画 → 撮影して SNS 共有。

**フェーズ 1.5** — 保存作品の詳細（「作品」タブの行をタップ）:

- **シーンシミュレーション** — 作品を別の状況に投影したときの見え方を
  プレビュー。照明（昼・曇り・夕方・夜の街灯）、壁の素材（コンクリート・
  シャッター・レンガ・白モルタル）と壁色、視距離（0.5〜30 m、水平画角
  60° の視点で作品が視野に占める大きさを再現）。完全にローカル
  （通信なし・写真素材なし、壁はコアの手続き生成）。
- **描画タイムラプス** — 作品が描かれた過程をストローク順・記録時刻
  どおりに再生（再生/一時停止・シーク・×1〜×8）。長い待ち時間は
  400 ms に圧縮。時刻を持たない旧（v1/v2）作品は描いた順に一定速度で
  再生する。
- **タイムラプス動画の書き出し** — 再生中の速度（×1〜×8）で描画過程を
  MP4（H.264、1080×810・30 fps）に書き出し、共有シートで SNS・写真
  ライブラリ・AirDrop などへ渡す。最後に完成した作品を 1.5 秒静止表示し、
  全体が 60 秒を超える長い作品は収まるよう自動で速度を上げる（画面に
  実際の速度を表示）。書き出し中は進捗表示とキャンセルが可能。完全に
  ローカル（通信なし、一時ファイルは次回の書き出しで削除）。

合成・照明・視距離のフレーミング・再生スケジュールはすべてコアの
`SavedWork`（`core/src/work.rs` → `raster.rs` / `scene.rs` /
`timelapse.rs`）が計算して RGBA 画像を返し、シェル
（`Graffix-AR/UI/WorkDetailView.swift`）は操作 UI と表示だけを行う。
動画書き出しの計画（各フレームが示す再生時刻・フレーム数・長さの上限と
自動加速・完成画の静止・偶数に丸めた映像サイズ・ビットレート）もコア
（`core/src/export.rs` の `TimelapseExport`）が決めてフレームを描き、
シェル（`Graffix-AR/UI/TimelapseVideoExporter.swift`）は AVFoundation で
エンコードして共有シートに渡すだけ。

## アーキテクチャ

```
┌─────────────────────────────┐
│ Swift shell (Graffix-AR/)   │  SwiftUI + RealityKit(ARView)
│  - ARSession → planes/      │  入力を集めて Engine#tick に渡す
│    camera/sceneDepth        │  結果(CanvasSnapshot)を描画するだけ
│  - テクスチャ描画 / UI      │  スプレー計算は一切しない
└──────────┬──────────────────┘
           │ UniFFI (GraffixCore.xcframework)
┌──────────┴──────────────────┐
│ Rust core (core/)           │  graffix-core — 全ロジック、Apple 非依存
│  - math: ray–plane / frame  │
│  - wall registry / lock     │
│  - distance (LiDAR 融合)    │
│  - spray engine v2 (mode/   │
│    cap/ellipse/drip/shake)  │
│  - undo/redo/clear          │
│  - canvas serialization v2  │
│  - geo: map spots/haversine │
│    filter+sort              │
└─────────────────────────────┘
```

- **`core/`** — Rust クレート `graffix-core`。`Engine` オブジェクトを
  UniFFI で Swift に公開。ホストで `cargo test` 可能。
- **`Graffix-AR/`** — iOS アプリ（iOS 27.0+, Swift 6）。ARKit の
  `ARSession`（垂直面検出 + LiDAR scene depth）から入力を取り、
  コアが返すダブ列を壁平面テクスチャとして描画。
- **`scripts/build-xcframework.sh`** — `aarch64-apple-ios` /
  `aarch64-apple-ios-sim` 向けビルド、UniFFI バインディング生成、
  `core/build/GraffixCore.xcframework` 組み立て。

## スプレーモデル v2（2モード制）

描画体験は VISION 仕様どおり 2 モード制。すべてコア (`core/src/spray.rs`)
が計算し、シェルは結果を描画・触覚に流すだけ。

### カジュアルライト（既定）

技術がなくてもそれらしく描けるプリセット方式。距離・角度・速度・攪拌の
影響は一切なく、ダブはプリセットの固定ソフト円:

| プリセット | 半径 | 不透明度 | エッジ(softness) |
|---|---|---|---|
| 太ブラシ | 5.0cm | 0.90 | 0.45 |
| 細ペン | 1.2cm | 0.95 | 0.15 |
| マーカー | 3.5cm | 1.00 | 0.06 |
| ふんわり | 9.0cm | 0.35 | 0.90 |

### リアルライト

実物のスプレーに近い物理モデル。

- **キャップ（ノズル）選択**: 噴射円錐・流量・接射半径・半径上限で
  パラメータ化（ファット / スタンダード / スキニー / ニードル）。
- **距離**: 半径 `radius(d) = clamp(r0 + cone_tan·d, r_min, r_max)`、
  不透明度 `clamp(0.85·flow·(0.35/d)², 0.04, 0.85)`（1/d² 減衰、
  `d ≤ 0.35m` はプラトー）。実装上は旧 v1 の半径/不透明度曲線を
  キャップごとに再パラメータ化したもの。
- **角度**: 入射角でダブは**楕円**になる。`major = radius/cosθ`
  （傾き方向に伸長、cos θ ≥ 0.2、major ≤ 0.9m でクランプ）、
  `minor = radius`。近い側（傾きと逆方向）が濃くなる密度勾配
  `gradient` をダブに持つ。
- **移動速度**: 0.25m/s まではフル強度、以降は `0.25/speed` 比例で
  薄くなる（下限 0.2）。速く振ると線がかすれて細くなる。
- **滞留**: 同じ場所に吹き続けると 0.15s ごとに再スタンプ
  （実物同様、止まると塗料が溜まる）。
- **ドリップ（垂れ）**: キャンバスを 4cm セルの密度場で管理し、
  セルの累積不透明度が閾値 4.0 の倍数を越えるたびにドリップを
  生成（長さは超過量で伸び、最大 0.45m）。壁ローカル -v に垂れる。
  ドリップはストロークの一部として保持・シリアライズされる
  （undo でストロークごと消える）。
- **缶振り**: `shake()` で `shake_level` += 0.34（3 回で満タン）、
  tick ごとに時定数 8s で減衰。レベルが低いとダブ不透明度が
  スペックル（かすれ）で劣化 — レベル 0 で係数 0.1〜0.6 の乱数、
  レベル 1 で無劣化。カジュアルは無効。

ダブ v2 モデル: `uv, radius_major, radius_minor, rotation, gradient,
opacity, softness`。距離推定は v1 と同じく幾何レイ–平面交差 `t` 主 +
LiDAR 融合（EMA 平滑化）。

## マップ画面（必須機能、VISION #11）

アプリは 3 タブ構成: **描く**（AR）/ **マップ** / **作品**。

- **公認ウォールスポット**（緑）— `Graffix-AR/Resources/spots.json` の
  同梱シードを読み込んでピン＋一覧表示。**現在のデータはすべて
  サンプル／プレースホルダ**（JSON の `notice` と各スポット名にも明記）
  で、実在する公認の壁を示さない。実データを入れるまでは「描いてよい
  壁」の根拠にしてはいけない。座標は渋谷近辺の例だが、機能は地域
  非依存（ロジックに地域の前提なし）。
- **他人の作品ピン**（紫）— `WorkPinProvider` プロトコル
  （`Graffix-AR/Map/WorkPinProvider.swift`）経由。現在は
  `MockWorkPinProvider` がサンプルピン（作者リンクもプレースホルダ）
  を返す。フェーズ2の作品インデックスは同プロトコルの実装に
  差し替えるだけで、マップ画面・詳細シートは無変更のまま使える
  （注入箇所は `RootView` の `MapScreen(...)` 初期化のみ）。
- **自分の作品ピン**（橙）— 「描く」画面の保存ボタンでキャンバス
  （GFXC）と現在地を `LocalWorkStore`（`Documents/works/`）に保存。
  `NSLocationWhenInUseUsageDescription` を追加済みで、拒否・未取得でも
  位置なしで保存できる（一覧には出るがピンは出ない）。
- **詳細シート** — スポット: 名称・種別・「ルート案内」（`MKMapItem`
  で Apple マップの徒歩ルートを開く）。他人の作品: タイトル・作者・
  外部 SNS リンク（ブラウザ）＋「レンズで見る」プレースホルダ
  （フェーズ2まで無効）。
- ピンの種別フィルタ・半径絞り込み・近い順ソートはコア `geo.rs`
  （`visibleMapSpots` / `haversineM`）が計算し、シェルはモデルを渡す
  だけ。位置権限が無いときは並び替えなしの全件表示で、初期表示は
  データの外接矩形に合わせる（地域ハードコードなし）。

### データフォーマット

`spots.json`（同梱シード、公認ウォール）:

```json
{
  "formatVersion": 1,
  "notice": "サンプルデータ — …実在する公認ウォールではありません…",
  "spots": [
    { "id": "sample-wall-001", "title": "…", "subtitle": "…",
      "latitude": 35.6595, "longitude": 139.7005 }
  ]
}
```

ローカル作品（`Documents/works/<id>.json` — `LocalWorkRecord`）:

```json
{ "id": "<uuid>", "title": "作品 …", "createdAt": 1731680400.0,
  "latitude": 35.65, "longitude": 139.70 }
```

`latitude`/`longitude` は省略可（位置なし保存）。キャンバス本体は
同ディレクトリの `<id>.gfxc`（`GFXC` v3 シリアライズ、下記）。

## 触覚フィードバック（Core Haptics）

コアは毎 tick `SprayStateF`（噴射中・流量・密度・モード・攪拌レベル）を
出力し、シェルはそれだけで触覚を駆動する:

- 噴射中は連続ハプティクス（intensity=流量、sharpness=密度）—
  `Graffix-AR/Haptics/SprayHaptics.swift`。
- デバイスを振ると加速度計で検出し `engine.shake()` + 攪拌ボールの
  カラカラ感 — `Haptics/ShakeDetector.swift`。
- キャップ/プリセット/モード変更はクリック感。

## レンダリング方針: texture-from-dabs

コアは壁ローカル座標 `(u, v)` のダブ列（楕円軸・回転・勾配・
不透明度・softness）とドリップ列を返す。シェルはそれを CoreGraphics
のグラデーションでオフスクリーンにラスタライズし — ダブは回転+伸縮
した放射グラデーション（密な核を近接側へオフセット）、ドリップは
頭から尾へ減衰する線形グラデーション — `TextureResource` を貼った
1 枚のクアッドとして壁の凍結フレーム上に配置する。ダブ単位の
エンティティを並べるより描画コストが一定で、見る角度に関わらず鮮明。
テクスチャはダブ/ドリップの外接矩形+余白で確保され、増分ラスタライズ
（追加分のみ描画）で、変更時のみ再生成する。

## キャンバスシリアライズ（v3, `GFXC`）

ストローク列は**壁ローカル座標**で保持され、ロック時に凍結した
キャンバスフレーム（origin/u/v）と共に小さなバイナリでシリアライズ
される（magic + version + CRC32）。v2 はダブを 9 float（uv, 長短半径,
回転, 勾配 uv, 不透明度, softness）+ ドリップ列で記録。**v1 の
既存ファイルもデコード可能**（4-float ダブを円形ダブとして読む）。
ワールド位置を含まないため、フェーズ 2 で位置/壁アンカーを付与して
アップロード・再アンカーできる設計。

v3（現行の書き出し）はタイムラプス用に、ストロークごとの任意の
タイミングブロック（開始時刻 + ダブ/ドリップごとのオフセット、ms）を
追加する。存在はストロークヘッダの旧予約バイトのフラグで示す。
時刻は「キャンバス時間」（エンジンが積算したフレーム時間）で、
壁時計の時刻は保存しない。**v1/v2 もそのままデコードでき**、時刻なし
（`timing: None`）として読まれる。旧作品を読み込んで描き足すと、
新しいストロークだけが時刻を持つ。フォーマット詳細は
`core/src/serialize.rs` 冒頭コメント参照。

## iOS での RealityView について

要件は「SwiftUI + RealityView」だが、本実装は `ARView`（RealityKit の
iOS ビュー）を `UIViewRepresentable` 経由で使っている。理由:

- iOS では AR セッションは同時に 1 つしか走らず、`RealityView` の
  `.spatialTracking` はセッションを内部で占有する。独自 `ARSession`
  の併設は不可。
- `SpatialTrackingSession` はコアが必要とする入力 — 検出平面アンカーの
  列挙（位置・サイズ）、毎フレームのカメラ姿勢、`sceneDepth` — を
  公開しない。`AnchorEntity(.plane)` は「条件に合うどれか 1 面」への
  追従のみで、全壁の列挙も extent も取れない。
- `ARView` は実 `ARSession` を持つ RealityKit ビューで、これら全てを
  供給できる。SwiftUI 内での RealityKit 描画面としての要件は満たす。

移行は `AR/ARViewContainer.swift` + 座標フィード部分に隔離済み。
将来 iOS の RealityView がアンカーデータを公開すれば差し替え可能。

## Mac でのビルド・実行手順

前提: Xcode 26+（iOS 27 SDK）、Rust 1.100+、実機（AR は Simulator 非対応）。

```bash
# 1. iOS ターゲットの追加（初回のみ）
rustup target add aarch64-apple-ios aarch64-apple-ios-sim

# 2. コアの XCFramework + Swift バインディングを生成
./scripts/build-xcframework.sh

# 3. Xcode で開く
open Graffix-AR.xcodeproj
# Signing & Capabilities で Development Team を設定し、
# スキーム "Graffix-AR" で実機に Run。
```

`core/build/GraffixCore.xcframework` と
`Graffix-AR/Generated/graffix_core.swift` はスクリプトで再生成できる
（バインディングはリポジトリにもチェックイン済み）。

## 動作確認済み / 未確認

**確認済み（Linux ホスト）**

- `cargo test`: 83 テスト全パス（フェーズ 1 の 44 + 1.5 の 39）— レイ–平面交差・点–平面距離、壁
  ロック選択、距離→半径/不透明度の単調性とクランプ、ダブ間隔、
  undo/redo/clear、シリアライズ v2 往復・v1 後方互換・破損検知、
  入射角→楕円軸の単調増大・勾配方向、速度による減衰、
  滞留→ドリップの閾値挙動、缶振り減衰+低攪拌かすれ（リアルのみ）、
  カジュアルプリセットの安定性、Engine 統合フロー、haversine 距離の
  既知都市間サニティ・マップスポットの種別/半径フィルタと距離ソート。
  フェーズ 1.5: GFXC v3 タイミング往復・順序違反/切り詰めの拒否・
  v2 手組みペイロードのデコード互換、エンジンの時刻スタンプ（待機の
  圧縮・読み込み後の描き足し）、タイムラインの合成スケジュール/
  進捗の単調性、ラスタライザ（楕円・回転・勾配・ドリップ・サブ
  ピクセル）、シーン（視距離→視野幅、昼夜の明暗・街灯の光だまり、
  夕方の斜光、壁色が塗料を変えないこと、シャッターのリブ・
  レンガ目地）、タイムラプス最終フレーム＝完成画像・巻き戻しの一致。
- `cargo check --target aarch64-apple-ios` / `aarch64-apple-ios-sim`:
  コアは iOS 向けにコンパイル可能（リンクは未実行）。
- UniFFI Swift バインディング生成（ホストビルドのライブラリから）。

**確認済み（GitHub Actions — `.github/workflows/ci.yml`）**

- Linux: `cargo test --locked`、`cargo clippy --all-targets --all-features
  -D warnings`、`cargo check --lib` を `aarch64-apple-ios` /
  `aarch64-apple-ios-sim` で。
- macOS（最新ランナー・最新安定版 Xcode）: `./scripts/build-xcframework.sh`
  で XCFramework を組み立て、チェックイン済みバインディングが生成物と
  一致することを確認し、`xcodebuild build -scheme Graffix-AR` を
  `generic/platform=iOS` と `generic/platform=iOS Simulator`
  （`CODE_SIGNING_ALLOWED=NO`）で実行 — **Swift シェル全体のコンパイル
  とリンクまで**。

依存更新は Dependabot（`.github/dependabot.yml`）が毎週月曜に
`core/` の Cargo 依存と GitHub Actions を main 宛 PR で提案する
（UniFFI はバインディング再生成が要るため別 PR）。Rust ツールチェーンは
CI の `dtolnay/rust-toolchain@stable` が毎回最新安定版を入れるため対象外。

**未確認（ビルドの成功は動作確認ではない）**

- アプリの実行そのもの（XCTest/UI テストは無い）。作品詳細の画面操作
  （シーン切替・視距離スライダー・タイムラプスの再生/シーク）、
  描画速度（実機でのフレームレート）、VoiceOver の読み上げ。
- コアのソフトウェアラスタライザと AR の `WallCanvasRenderer` の見た目の
  一致（同じモデルを実装しているが、並べての目視比較はしていない）。
- - 位置情報許可フロー（プロンプト文言、拒否時のマップ挙動、保存時の
  ワンショット測位）は実機確認が必要。
- 実機 AR: 面検出、LiDAR 深度、描画外観（テクスチャの向き・反転、
  ハイライト表示、楕円ダブの回転方向・勾配方向、ドリップの向き、
  スナップショット内容）は実機確認が必要。
- 実機触覚: ハプティクス強度/鋭さの体感、シェイク検出の閾値は
  実機調整が必要。
- ARKit `planeExtent.rotationOnYAxis` の符号、RealityKit 平面メッシュ
  の UV 向き — キャンバス/ハイライトが上下反転する場合は
  `WallCanvasRenderer` の基底マッピングを反転させる。

## 開発する

開発は Devin が担当します。進め方は [AGENTS.md](AGENTS.md)、プロジェクトの事実は
[docs/project.md](docs/project.md)、旧運用の保管と戻し方は
[docs/legacy-ops](docs/legacy-ops/README.md) を参照してください。必須の確認は
`.github/workflows/ci.yml` の 2 ジョブ（内容は上記「動作確認済み / 未確認」）です。

`project.pbxproj` は同期フォルダ group 方式（objectVersion 77）で、
`Graffix-AR/` 配下のファイルは自動でアプリターゲットに含まれます。
