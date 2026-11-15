# フェーズ 2 設計メモ（提案 — オーナー判断待ち）

このメモは実装を含みません。[`docs/concept/VISION.md`](../concept/VISION.md)
にあるフェーズ 2（作品の保存・共有基盤、位置／壁アンカーで他人の作品を
引ける軽いインデックス、ユーザー情報は持たない）について、未決事項ごとに
現実的な選択肢と、それぞれの代償を並べたものです。

- **何も決定していません。** 「提案」と書いた箇所は判断材料で、決めるのは
  オーナーです。決定が必要な項目は末尾の[判断事項一覧](#8-判断事項一覧)
  にまとめています。
- プラットフォームの機能や制約について書いたことには、公式ドキュメントの
  出典を付けています（[参考文献](#9-参考文献)、2026-11 に閲覧）。
  ドキュメントで確認できなかったことは「**要確認**」と書いています。
- 実機での検証はしていません（測位精度の実測、geotracking の対応地点の
  確認など）。精度について書いた内容は、ドキュメントの記述か一般的な
  性質の説明で、測った値ではありません。
- 前提として、VISION の「カルチャーへの敬意」「地域を限定しない」
  「アプリ側にユーザー情報を持たせすぎない」を判断の軸にしています。

## 1. 現状の土台（このブランチの実装から）

フェーズ 2 の設計に直接関わる、現行コードの性質です。

| 項目 | 現状 | フェーズ 2 への含意 |
|---|---|---|
| キャンバス形式 | `core/src/serialize.rs` の GFXC v3。ストロークは**壁ローカル (u, v) [m]**。ヘッダに描画時の `PlaneFrame`（origin / u / v）を持てる。CRC32 付き。 | 世界位置を持たないので、アンカーは別に付ける必要がある（設計どおり）。 |
| `PlaneFrame` の意味 | `PlaneFrame::facing`（`core/src/math.rs`）は、`v` をできるだけ world-up（重力方向）に合わせ、`u` を水平にとる。origin は、ロックした時点の ARKit 平面の中心点。 | ストロークは**重力に揃った壁ローカル座標**なので、再アンカーで必要なのは「壁の平面」「その平面上の原点」「法線の向き」だけで、回転を推定し直す必要はない。一方で origin と u/v は**描いたセッションのワールド座標**でしか意味を持たない。 |
| ワールド座標 | `ARSessionCoordinator` は `ARWorldTrackingConfiguration`（既定の `worldAlignment = .gravity`）を使う。y 軸が重力、原点はセッション開始時の端末位置、水平方向の向きは端末の初期の向きで決まる [A6]。 | 保存している frame からは、壁が**どの方角を向いているか**も**地球上のどこにあるか**も分からない。フェーズ 2 では、壁の位置と法線の方位を別の手段で記録する必要がある。 |
| 作品の位置 | `LocalWorkRecord`（`Graffix-AR/Data/LocalWorkStore.swift`）は保存時の緯度経度を任意で持つ。測位は `kCLLocationAccuracyNearestTenMeters` の 1 回だけ。 | インデックスの位置（どの辺りか）としては使えるが、壁に重ねる位置としては粗すぎる。 |
| 他人の作品ピン | `WorkPinProvider` プロトコル（`Graffix-AR/Map/WorkPinProvider.swift`）の `workPins(near: GeoPointF?) async -> [MapSpotF]`。差し込んでいるのは `RootView` の 1 か所だけ。 | フェーズ 2 のインデックスは、このプロトコルを実装すれば地図にそのまま載る。地図に必要なのは「軽いメタデータ」だけで、GFXC 本体はレンズで見るときに取りに行けばよい。 |
| 作者リンク | `MapSpotF.author_url: Option<String>`（`core/src/geo.rs`）は自由形式の URL。 | 外部 SNS のリンク形式（§4）が決まったら、ここを構造化するかどうかを決める。 |
| 地図の計算 | 絞り込み・並べ替え・距離は `geo.rs`（`visible_map_spots` / `haversine_m`）。 | インデックスのセル計算、寿命、希少性の判定など、計算は同じようにコアに置ける。 |

## 2. 未決事項 1：アンカー方式

「アンカー」には役割が 2 つあり、分けて考えると選択肢を比べやすくなります。

1. **インデックス用の位置**：地図やインデックスで「この辺りにある」と
   引くための位置。誤差は数十 m あってもよい。
2. **重ね合わせ用の配置**：レンズで見たとき、作品を**その壁の、その場所**に
   重ねるための配置。誤差は数 cm〜数十 cm に抑えたい。

インデックス用の位置は、どの方式を選んでも GPS（端末の位置情報）で
足ります。決める必要があるのは主に 2 の方式です。

### 選択肢

#### A. 自前方式：GPS ＋ 壁平面（ユーザーが合わせる）

保存時に、作品の緯度経度（インデックス用）と、壁の法線の方位を記録します。
方位は可能なら端末のコンパスから取ります。見る側は、地図のピンを頼りに
現地へ行き、既存の壁ロックで壁を選んでから、作品の位置をドラッグや
ガイドで合わせます。

- **精度と対応範囲**：対応範囲は全世界（GPS が使える場所）。精度は
  GPS の誤差に左右されるので、自動で cm 単位に重ねることはできず、
  ユーザーが合わせる前提になる。合わせた後の見た目は、既存のロック
  （LiDAR ＋レイと平面の交差）の精度になる。
- **コスト**：外部 AR サービスは不要。必要なのは位置付きのメタデータ
  ストア（§3）だけ。
- **プライバシー**：保存するのは作品の座標と方位だけ。カメラ画像や
  周囲の 3D 特徴は外に出ない。VISION のデータ方針に最も近い。
- **モデレーション**：作品の座標は粗いので、「どの壁に描かれたか」を
  厳密には特定できない。私有の壁などを除外するルールを座標で
  機械的に判定しにくい。
- **コアとの適合**：最も素直。`PlaneFrame` は重力に揃っているので、
  アンカーとして「地理座標＋法線の方位＋壁上のオフセット」を足すだけで
  よい。計算（方位から壁の候補を選ぶ、合わせた後の壁ローカルへの変換）
  はコアに置け、ホスト上でテストできる。
- **弱点**：「通りかかったら他人の作品が自然に壁に乗っている」という
  発見体験（VISION #1 Graffix レンズ）は弱くなる。

#### B. ARKit の位置アンカー（`ARGeoTrackingConfiguration` / `ARGeoAnchor`）

Apple のマップ用に事前撮影された「ローカライゼーション画像」と、
カメラ映像を照合して、地理座標と AR 空間を対応させます [A1]。

- **対応端末**：A12 以降のチップで、セルラー（GPS）機能がある端末に限る。
  GPS を持たない Wi-Fi モデルの iPad は対象外 [A1][A3]。
- **対応範囲**：「20 か国以上の特定エリア」で、日本の大都市圏を含む [A1]。
  特定の地点で使えるかは、実行時に `checkAvailability` で確認する [A2]。
  - **屋外専用**で、ネットワーク接続が必要 [A1][A2]。
  - ローカライゼーション画像は**車で通れる公道**から撮影されていて、
    **ゲートの中や歩行者専用の場所は含まれない** [A1][A3]。グラフィティの
    壁は路地や公園、高架下に多いので、対応範囲に入らない地点が多い
    可能性がある（実地調査で**要確認**）。
- **精度**：状態としては `low` / `medium` / `high` の 3 段階が報告される。
  Apple は、精度が `low` のときは表示を誤差に寛容なものにすることを
  推奨している [A5]。ドキュメントに m 単位の保証値はない（**要確認**）。
  「作品を壁に沿わせる」用途に足りるかは、実機での実測が必要。
- **セッション構成への影響**：現在の `ARWorldTrackingConfiguration` から、
  geotracking 用の構成に切り替える必要がある。geotracking 構成でも
  `planeDetection` は使える [A7]。ただし LiDAR の `sceneDepth` を
  併用できるかはドキュメントで確認できなかった（**要確認**。距離推定は
  レイと平面の交差だけでも動く設計なので、劣化はするが代替はある）。
  利用できるのは背面カメラだけ [A3]。
- **コスト**：OS 標準の機能なので利用料はない。端末はローカライゼーション
  画像をダウンロードするので、データ通信量について利用者に知らせる
  ことが推奨されている [A4]。
- **プライバシー**：作品側に保存するのは地理座標（緯度・経度・高度）
  だけで、`getGeoLocation(forPoint:)` で壁上の点を地理座標に変換できる
  [A8]。Apple 側でカメラ映像がどう扱われるかは、今回読んだ範囲では
  確認できなかった（**要確認**）。
- **コアとの適合**：アンカーの中身は「壁上の基準点の地理座標＋法線の方位」
  で、A と同じ型に収まる。A の上位互換として扱えるので、両方を実装
  しても形式は 1 つで済む。

#### C. ARKit の `ARWorldMap`（周囲の特徴点マップの保存・共有）

描いたセッションの空間認識をシリアライズしておき、同じ場所で再ローカライズ
（位置の再特定）します [A9][A10]。

- **精度と対応範囲**：同じ物理環境なら、保存したアンカーを同じ位置に
  戻せる [A9]。合わせられない場合、セッションは relocalizing の状態のまま
  抜けない [A10]。照明や季節、周りの物の変化にどこまで耐えるかは、
  ドキュメントに記述がない（**要確認**）。屋外で日数が経ったマップは
  照合に失敗しやすいと考えられる（推測）。保存前に
  `worldMappingStatus` を確認することが推奨されている [A11]。
- **コスト**：作品ごとにマップのデータ（数 MB 規模になりうる。**要実測**）
  を保存・配信する必要がある。生成できるのは world tracking のセッション
  だけで、geotracking と同時には使えない [A11]。
- **プライバシー**：周辺環境の特徴点がサーバーに上がる。人の情報ではないが、
  場所の詳細な空間データになる。
- **コアとの適合**：マップは Apple 固有の不透明なデータで、コアでは扱えない。
  シェル側の添付データとして別に持つことになる。

#### D. Google ARCore Geospatial API（iOS SDK）

Street View の画像から作られた VPS（Visual Positioning System）で端末の
位置を特定し、WGS84 / Terrain / Rooftop の 3 種類のアンカーで地理座標に
コンテンツを置きます [G1]。

- **対応範囲**：Google Street View が撮影されている範囲 [G1]。VPS が
  使えるかは座標ごとに事前に確認できる [G2]。VPS が使えない場所でも、
  上空が開けていれば GPS で精度を出せる場合がある、と書かれている [G2]。
- **コスト・運用**：Google Cloud のプロジェクトが必要。推奨される keyless
  認証では、**自前のサーバーで JWT に署名する**必要がある [G3]。
- **プライバシー**：アプリ内で「Google がセンサーデータ（カメラ、位置）を
  処理する」旨を表示することが義務になっている [G4]。カメラ映像を
  第三者が処理することになり、VISION のデータ方針（アプリは作品と外部 SNS
  への参照だけを持つ）と比べると、外部への依存とデータの流れが一段
  大きくなる。
- **コアとの適合**：アンカーの中身は地理座標なので、A・B と同じ型に
  収まる。SDK はシェル側で ARKit に組み込む形になる [G5]。

#### E. Google ARCore Cloud Anchors

作品ごとに、周囲の視覚的な特徴を Google にホストし、ID で解決
（同じ位置の再現）します [G5][G6]。

- **用途の適性**：Google 自身が「部屋程度の規模の AR に最適」と書いている
  [G6]。ホストする前に、対象の周りを複数の角度から撮っておく必要がある
  [G6]。
- **寿命と認証**：API キーで認証すると、アンカーの最長寿命は **24 時間**。
  それより長く残すには keyless 認証が必須 [G5]（最長でいつまで残せるかは
  **要確認**）。
- **クォータ**：ホストは 1 分あたり 30 リクエスト、解決は 1 分あたり 300
  リクエストまで（IP アドレスとプロジェクトの単位）[G5]。
- **プライバシー**：視覚データを Google のサーバーに送る [G5]。D と同じく、
  利用の表示が義務 [G4]。

### 比較（要約）

| | A 自前（GPS＋壁） | B ARKit Geo | C WorldMap | D ARCore Geospatial | E Cloud Anchors |
|---|---|---|---|---|---|
| 対応範囲 | 全世界 | 対応都市の公道沿い・屋外 [A1] | どこでも（同じ環境に限る） | Street View の範囲 [G1] | どこでも |
| 自動で重なるか | ×（手で合わせる） | ○（精度は 3 段階 [A5]） | ○（環境が変わると失敗） | ○ | ○ |
| 外部サービス | なし | Apple（OS 標準） | なし | Google（JWT 用サーバー [G3]） | Google |
| カメラデータの外部送信 | なし | **要確認** | なし（ただしマップをアップロード） | あり（表示義務 [G4]） | あり [G5] |
| アンカーデータの大きさ | 数十 byte | 数十 byte | 数 MB 規模（**要実測**） | 数十 byte | ID のみ |
| コアの型との適合 | ◎ | ◎（A と同じ型） | △（不透明な添付） | ◎ | ○（ID） |

### 提案（オーナー判断待ち）

- **インデックス用の位置**は、全方式に共通で GPS の座標を使い、さらに
  コアで粗いセル（グリッドのキー）に丸める案。サーバーには丸めたセルで
  問い合わせ、端末側の `geo.rs` で絞り込む。
- **重ね合わせ**は、**A を全世界での基本**にし、`checkAvailability` が
  true を返す地点だけ **B で自動の重ね合わせ**に格上げする、二段構えの案。
  - 理由：地域を限定しない（VISION）、外部サービスを増やさない（データ
    方針）、アンカーの型を 1 つにできる（A と B は同じ「地理座標＋方位＋
    壁上のオフセット」）。
  - D と E は、精度や対応範囲の面で魅力がある一方、カメラデータを第三者が
    処理することと、自前で JWT サーバーを持つ必要があることから、
    データ方針との兼ね合いをオーナーが判断する項目として残す。
    C は、屋外での耐久性が不明なので、現時点では候補から外す提案。
- **決める前の検証（提案）**：実機の試作で、(1) 代表的な合法ウォールの
  候補地点で `checkAvailability` の結果、(2) B の精度が `medium` / `high`
  に達するまでの時間と、壁上での実際のずれ、(3) geotracking 構成で
  `sceneDepth` が使えるか、を測る。

### コア側の形式の選択肢（参考）

アンカーを作品データのどこに持つか。

1. **GFXC にアンカーのブロックを足す（v4）**：1 ファイルで完結する。
   ただし、GFXC の版を上げることになり、旧版との互換テストが増える。
2. **GFXC とは別のレコードに持つ**（`LocalWorkRecord` の拡張や、
   サーバーのメタデータ）：GFXC v3 は変えずに済む。インデックス
   （軽いメタデータ）と本体（GFXC）を分けて配信する構成とも合う。

提案は 2。インデックスに要るのはメタデータだけで、GFXC はレンズで見る
ときに取りに行けばよいため。アンカーの型とその変換は、どちらの案でも
コアに置いて単体テストします。

## 3. 未決事項 2：作品データの置き場所・公開範囲・モデレーション

### 3.1 置き場所の選択肢

#### A. CloudKit の公開データベース

- **読み書きの条件**：読み出しは、端末に iCloud アカウントが無くても
  できる [C1]。書き込み（レコードの保存）には iCloud へのサインインが
  必要 [C2]。利用者は、自分が作ったレコードに書き込める [C1]。
  データはアプリの iCloud ストレージのクォータに数えられる [C1]。
- **位置での検索**：`distanceToLocation:fromLocation:` で半径検索が
  できるが、**位置のインデックスの解像度は 10 km 以上** [C3]。
  地図の「近く」の検索に使うには粗いので、セルのキーを文字列で持つ
  などの工夫が要る。
- **スキーマ**：本番環境に配布した後は、追加の変更しかできない [C2]。
- **プライバシー**：作者の識別は iCloud の不透明なユーザー記録
  （ユーザーレコード）になり、アプリ自身は ID を発行しない。
- **モデレーション**：CloudKit Dashboard でロールを割り当て、アクセスを
  制限できる [C1]。ただし、通報の受け付けや自動フィルタ、寿命の計算
  など、サーバー側の処理は別に用意する必要がある（CloudKit だけで
  完結するかは**要確認**）。
- **コスト**：自前のサーバーを運用しなくて済む。

#### B. 自前のバックエンド（マネージドな BaaS、または自前 API ＋オブジェクトストレージ）

- 通報の受け付け、フィルタ、寿命や延命の計算（#9）、通知（#5）、
  希少性の検証（§6）を、すべてサーバー側で強制できる。
- 位置での検索は自由に設計できる（セルのキー、空間インデックス）。
- 運用コストと保守が発生する。料金は事業者ごとに異なるので、
  このメモでは比較しない（選ぶ段階で**要確認**）。
- 端末の識別には App Attest / DeviceCheck が使える（§3.3）。アカウントを
  作らずに、不正な利用を抑えられる。

#### C. サーバーを持たない（端末間の共有、リンク共有だけ）

- 「位置で他人の作品を引けるインデックス」というフェーズ 2 の要件を
  満たせない。比較のための参考として挙げるだけ。

### 3.2 公開範囲の選択肢

「違法な描画のデジタル版にならない」ためのルールの選択肢です。

| 案 | 内容 | 長所 | 短所 |
|---|---|---|---|
| P1 合法ウォール限定 | 公開できるのは公認ウォールのスポット（`spots.json` →フェーズ 2 で実データ）の範囲内だけ。それ以外は自分だけに見える。 | 「景観を守る」がそのまま機能になる。モデレーションの範囲が狭い。 | 公認ウォールの実データが要る（現在の同梱データは**すべてサンプル**）。発見の範囲が狭い。 |
| P2 どこでも公開（除外区域あり） | 原則どこにでも公開できる。私有地、学校、宗教施設などの除外区域は運営が管理し、通報で追加する。 | 発見の範囲が広い。 | 他人の建物に「描く」体験を後押しするおそれがあり、文化への敬意やエチケットとの緊張がある。除外区域のデータの整備が重い。 |
| P3 段階公開 | 公開するとまずは近くの人だけに見え、リアクションが付くと広がる（#9 と連動）。 | 代謝の仕組みと合う。 | ルールが複雑になる。 |
| P4 非公開・リンク公開 | 地図には出さず、リンクを知っている人だけが見られる。 | 安全。 | インデックスの意義が薄れる。 |

### 3.3 モデレーション（App Store の要件と、データ方針との緊張）

App Store Review Guideline 1.2 は、利用者が作るコンテンツを扱うアプリに、
次の 3 つを求めています [R1]。

1. 不適切なものが投稿されないようにフィルタする手段
2. 不快なコンテンツを通報する仕組みと、迅速な対応
3. 嫌がらせをする利用者をサービスからブロックする機能

**3 の「利用者をブロックする」には、作者を識別する何らかの ID が要ります。**
これは「ユーザー情報を持たない」という方針と緊張する点です。
両立させる選択肢は次のとおりです。

- **M1 作者の公開鍵だけを持つ**：作品に、端末で作った匿名の鍵
  （公開鍵）を付け、ブロックや凍結はその鍵の単位で行う。名前やメールは
  持たない。App Attest で「正規のアプリが作った鍵」であることを
  サーバーで確認できる [D2]。ただし、App Attest の鍵は**アプリの
  再インストールや機種変更、バックアップからの復元で失われる** [D2]
  ので、ブロックを逃れるために再インストールされる可能性は残る。
- **M2 M1 に DeviceCheck を足す**：端末ごとに 2 ビットの状態を Apple が
  保持してくれる [D1]（例：「凍結中」）。再インストールしても残るので、
  ブロックを逃れにくくなる。保持できる情報は 2 ビットだけ。
- **M3 iCloud のユーザーレコード**（3.1 の A を選んだ場合）：識別子は
  Apple の不透明なもの。
- **M4 外部 SNS での認証**（§4 の L3）：なりすましは防げるが、アプリが
  トークンを持つことになり、4.8 の要件（§4）も関わってくる。

フィルタについて：

- スプレーで描いた作品はベクター（ダブの列）なので、画像として判定する
  には、コアのラスタライザ（`raster.rs`）で画像にしてから、サーバー
  側で判定することになる（判定に何を使うかは**要決定**）。
- 端末上の `SensitiveContentAnalysis` は、利用者が「センシティブな
  コンテンツの警告」やスクリーンタイムの設定を有効にしている場合に、
  その意思を尊重して表示を抑えるための仕組み [S1]。**投稿の審査には
  使えない**（審査の用途はドキュメントの想定外）と読める。
- インポート画像やステッカー（§6）は、第三者の知的財産や不適切な画像が
  持ち込まれやすい経路になる。1.2 は知的財産の侵害にも言及している [R1]。

### 提案（オーナー判断待ち）

- **置き場所**：通報対応、寿命、希少性、通知をサーバー側で強制する必要が
  あるので、**B（自前のバックエンド）**を提案。A（CloudKit）は運用を
  軽くできる一方で、位置の解像度（10 km 以上 [C3]）、書き込みに iCloud
  のサインインが必要なこと [C2]、サーバー処理を別に用意する必要が
  あることが制約になる。
- **公開範囲**：まずは **P1（合法ウォール限定）**で始め、P3（段階公開）
  への拡張の余地を残す案。VISION の「景観を守る」「カルチャーへの
  敬意」に最も沿う。ただし、公認ウォールの実データの入手方法を
  オーナーが決める必要がある。
- **モデレーション**：**M1 ＋ M2**（匿名の公開鍵と DeviceCheck）、
  通報キュー、運営による非公開化の組み合わせを提案。保持するデータは
  作品、アンカー、作者の公開鍵、外部 SNS への参照に限る。

### 作品レコードの形（どの案でも共通の叩き台）

```text
WorkIndexEntry（地図・インデックス用、軽量）
  id, cell_key, lat, lon, title, author_ref(§4), created_at,
  visibility_until(§5 #9), anchor_kind(geo_manual|geo_arkit|…)
WorkBody（レンズ用、必要になってから取得）
  gfxc_v3_bytes, anchor{lat, lon, alt?, heading_deg, wall_offset_uv}
```

`WorkPinProvider` の実装は `WorkIndexEntry` を `MapSpotF` に写すだけで
済みます。

## 4. 未決事項 3：紐づける外部 SNS の優先順位とリンク形式

### 優先順位（判断材料）

VISION に挙がっているのは X と Instagram です。ほかの候補
（TikTok、YouTube など）を含めて、優先順位は**オーナーが決める項目**です。
技術面での違いは次のとおりです。

- **リンクを張るだけ**（作品から作者のプロフィールへ飛ぶ）なら、
  どの SNS でも https のプロフィール URL を開くだけで、SDK は要らない。
  今の詳細シートと同じく、ブラウザで開く形になる。
- **写真を投稿する導線**（フェーズ 3）は、今の `ShareSheet`
  （`UIActivityViewController`）なら、SNS ごとの SDK なしで全部に対応
  できる。Instagram のストーリーズへ直接共有するには、Facebook の
  アプリ ID、URL スキーム `instagram-stories://share`、
  `LSApplicationQueriesSchemes` の登録が必要 [M1]。

### リンク形式の選択肢

| 案 | 内容 | 長所 | 短所 |
|---|---|---|---|
| L1 自由形式の URL | 今の `MapSpotF.author_url` のまま | 実装は最小 | 任意の URL を入れられるので、フィッシングに使われるおそれがある。表示がそろわない。 |
| L2 SNS の種類＋ハンドル | `{platform: x\|instagram\|…, handle}` を持ち、URL はコアが組み立てる | 検証（使える文字）と表示をそろえられる。リンク先のドメインを固定できる。 | 自分のアカウントであることの証明にはならない（なりすましは通報で対応）。 |
| L3 OAuth で連携 | X や Instagram でログインして、アカウントの持ち主であることを確認する | なりすましを防げる | アプリがトークンを持つことになり、データ方針と緊張する。外部 SNS のログインを**主なアカウント**に使う場合、同等の別のログイン手段（メールアドレスを非公開にできるものなど）を併せて提供する義務がある [R2]。アカウントを作れるなら、アプリ内でアカウントを削除できる必要もある [R3]。 |

- 各 SNS のプロフィール URL の書式（`https://x.com/<handle>` など）が
  将来も変わらないかは、公式ドキュメントで確認できなかった
  （**要確認**）。L2 なら、書式が変わってもコアの 1 か所を直すだけで済む。

### 提案（オーナー判断待ち）

**L2** を提案。アカウントも外部 SNS のトークンも持たずに、リンク先の
ドメインを固定でき、データ方針に沿います。なりすましには通報と
運営の対応で当たります（§3.3）。優先する SNS はオーナーが決めます。
L2 なら対応する SNS の追加は列挙型に値を 1 つ足すだけなので、
順番を後から変えても大きな手戻りはありません。

## 5. 関連機能：#5 通知、#9 作品の寿命とバフ

### #5 通知

- **近くで新しい作品が完成した**：
  - N1 **サーバーからのプッシュ**（APNs）：端末のトークンをサーバーに
    登録する [P1]。どのセルを購読しているかで、端末のおおよその位置が
    サーバーに分かる。
  - N2 **端末での地域監視**：Core Location の条件監視では、1 つのアプリが
    同時に監視できるのは**最大 20 条件** [L1]。位置の精度を「おおよそ」
    にしか許可されていないと、地域監視は使えない [L2]。端末が近くの
    20 セルを監視し、入ったときにインデックスを問い合わせて、ローカル
    通知を出す。サーバーには位置が分からない。
- **有名アーティストの新作**：フォローしている関係をどこに持つか。
  - F1 **端末だけに持つ**（フォローしている作者の公開鍵の一覧）：
    サーバーにソーシャルグラフを持たずに済む。アプリを開いたとき
    （またはバックグラウンドでの取得）に差分を見る。
  - F2 **サーバーに購読を持つ**：プッシュですぐ届くが、
    「誰が誰をフォローしているか」がサーバーに残る。
  - F3 **外部 SNS のフォローに任せる**：アプリでは何もしない。
- **提案（判断待ち）**：N2 ＋ F1 を最初の候補として提案。いずれも、
  サーバーに人の情報を持たない構成です。

### #9 作品の寿命とバフ

- 「時間とともに薄れ、リアクション（❤🔥）で延命される」計算は、
  **コアの純粋関数**として実装し、単体テストできる。例：
  `visibility(created_at, reactions[], now) -> (alpha, until)`。
  サーバーも同じ式で `visibility_until` を計算する（コアを
  サーバーで使えるかは、バックエンドの構成次第）。
- リアクションは匿名の件数として集計し、1 つの鍵（§3.3 の M1）につき
  1 作品 1 回などの制限で、水増しを防ぐ。
- **上塗り（バフ）**：同じ壁の同じ場所に新しい作品が置かれたら、古い
  作品が隠れる。この判定には、重ね合わせの精度が必要（§2 の B 以上）。
  A（手で合わせる）だと、同じ壁であることの判定が粗くなる。
- マネタイズ（掲載期間の延長）は、App 内課金（StoreKit）が前提になる
  [K1]。抽選型の商品を売る場合は、事前にそれぞれの確率を表示する必要が
  ある [R4]。

## 6. フィンガーライト／ステッカー：バックエンド前にローカルで作れる範囲

フェーズ 2 のバックエンドが無くても作れる部分と、作れない部分を分けて
並べます。**どこまで先行して作るかはオーナーが決めます。** 各段階は、
前の段階に積み上げる形です。

### 段階 S0：フィンガーライトの入力と、壁への貼り付け（ローカルで完結）

- **入力**：画面に指で描く。手段は 2 つ。
  - `PKCanvasView`（PencilKit）：指でも描ける [K2]。ただし、描画モデルが
    PencilKit 側にあるので、「計算はコア」という原則から外れる。
    結果を画像として持つことになる。
  - **自前で取ったタッチの座標をコアに渡す**：ストロークの生成（太さ、
    なめらかにする処理）をコアで行い、既存の `Stroke` / `Dab` に
    落とせる。**提案はこちら**。GFXC v3 に変更を加えずに保存でき、
    タイムラプスとシーンシミュレーションもそのまま使える。
- **「ペッと貼る」**：既存の壁ロックとレイの交差で貼る位置を決め、
  コアで壁ローカルの配置（中心、拡大率、回転）に変換する。
- **バックエンドは不要**。保存先は既存の `LocalWorkStore`。

### 段階 S1：インポート（写真から）

- 選ぶのは `PhotosPicker`。PNG と JPEG は標準で読み込める [K3]。
- **GFXC v3 はストロークしか持てない**ので、選択肢は次の 2 つ。
  1. ラスター画像のレイヤーを GFXC に足す（v4）。
  2. 取り込んだ画像を、コアでダブの列にベクター化する。見た目は
     「スプレーらしく」変わる。
  どちらにするかは、文化的にどう見せたいかを含めて**オーナーが判断**。
- **モデレーションの経路になる**（§3.3）。ローカルでだけ使う間は
  問題が小さいが、公開する段階では審査の仕組みが前提になる。

### 段階 S2：ステッカー（同梱の素材を貼る）

- 素材を同梱して、S0 と同じ仕組みで貼る。**素材を誰が作るか、
  どんな柄にするか**はオーナーの判断（カルチャーへの敬意の観点で
  特に重要）。
- 枚数制限や時間制限の**ルールの計算**（残り枚数、回復時刻）は、
  コアの純粋関数としてローカルで作り、テストできる。

### 段階 S3：希少性ルールの「ローカル版」

- **同じものは 1 つまで**（インポートとフィンガーライト）：コアで作品の
  フィンガープリント（正規化したストロークのハッシュ。インポート画像は
  知覚ハッシュ）を計算し、端末の中で「もう貼ったか」を判定する。
- **限界**：ローカルでしか判定しないので、再インストールや別の端末で
  すり抜けられる。**全体で 1 つまで、を保証するにはサーバーの
  インデックスが必要**（フェーズ 2）。ローカル版は、体験を試すための
  プロトタイプ、という位置づけになる。

### バックエンドが無いと作れないもの

- 全体での一点物の保証、端末をまたいだ枚数の制限（App Attest /
  DeviceCheck と組み合わせる [D1][D2]）
- インポート画像の公開前の審査
- ステッカーパックの課金（App 内課金 [K1]。課金した状態をサーバーで
  検証するかどうか）
- 他人のステッカーやフィンガーライト作品を、レンズや地図で見ること

### 提案（オーナー判断待ち）

S0 → S2 → S3 の順にローカルで先行し、S1（インポート）は審査の仕組みが
決まるまで保留にする案。S0 と S2 は既存のコア（`Canvas` / `Stroke`、
壁ロック、GFXC v3）の延長で作れ、GFXC の版を上げる必要がありません。

## 7. 実装順の一案（参考）

1. アンカーの型（地理座標＋方位＋壁上のオフセット）と、インデックス用の
   セルの計算を、コアに入れて単体テストする（バックエンドは不要）。
2. `LocalWorkRecord` にアンカーを保存する（方式 A）。
3. バックエンドを決め、`WorkPinProvider` を実装する（地図に他人の作品が
   載る）。
4. 方式 A でレンズの重ね合わせを作り、対応地点だけ B に格上げする。
5. 寿命（#9）と通知（#5）を入れる。

## 8. 判断事項一覧

| # | 判断事項 | 選択肢 | このメモの提案 |
|---|---|---|---|
| D1 | 重ね合わせのアンカー方式 | A 自前 / B ARKit Geo / C WorldMap / D ARCore Geospatial / E Cloud Anchors（組み合わせも可） | A を基本、使える地点だけ B（実機での測定の後に確定） |
| D2 | D/E（Google が処理）を許容するか | 許容 / 不許容 | データ方針との兼ね合いでオーナーが判断 |
| D3 | 作品データの置き場所 | CloudKit / 自前のバックエンド | 自前のバックエンド（具体的な事業者は**要選定**） |
| D4 | 公開範囲 | P1 合法ウォール限定 / P2 どこでも / P3 段階公開 / P4 リンク公開 | P1 で始め、P3 へ拡張 |
| D5 | 公認ウォールの実データの入手方法 | （オーナーが決める） | — |
| D6 | 作者の識別（ブロック用） | M1 公開鍵 / M2 ＋DeviceCheck / M3 iCloud / M4 SNS 認証 | M1 ＋ M2 |
| D7 | 外部 SNS の優先順位 | X / Instagram / ほか | オーナーが決める |
| D8 | SNS リンクの形式 | L1 自由 URL / L2 種類＋ハンドル / L3 OAuth | L2 |
| D9 | 通知の方式 | N1 プッシュ / N2 端末で地域監視、F1 / F2 / F3 | N2 ＋ F1 |
| D10 | ローカルで先行する範囲 | S0〜S3 の任意の組み合わせ | S0 → S2 → S3、S1 は保留 |
| D11 | インポートの表現 | ラスターのレイヤー（GFXC v4）/ ベクター化 | オーナーが判断 |
| D12 | ステッカー素材の制作体制 | （オーナーが決める） | — |

## 9. 参考文献

すべて 2026-11 に閲覧した公式ドキュメントです。

**Apple — ARKit**

- [A1] ARGeoTrackingConfiguration — https://developer.apple.com/documentation/arkit/argeotrackingconfiguration
- [A2] checkAvailability(completionHandler:) — https://developer.apple.com/documentation/arkit/argeotrackingconfiguration/checkavailability(completionhandler:)
- [A3] Tracking geographic locations in AR — https://developer.apple.com/documentation/arkit/tracking-geographic-locations-in-ar
- [A4] ARGeoAnchor — https://developer.apple.com/documentation/arkit/argeoanchor
- [A5] ARGeoTrackingStatus.Accuracy — https://developer.apple.com/documentation/arkit/argeotrackingstatus/accuracy-swift.enum
- [A6] ARConfiguration.WorldAlignment.gravity — https://developer.apple.com/documentation/arkit/arconfiguration/worldalignment-swift.enum/gravity
- [A7] ARGeoTrackingConfiguration.planeDetection — https://developer.apple.com/documentation/arkit/argeotrackingconfiguration/planedetection
- [A8] getGeoLocation(forPoint:completionHandler:) — https://developer.apple.com/documentation/arkit/arsession/getgeolocation(forpoint:completionhandler:)
- [A9] ARWorldMap — https://developer.apple.com/documentation/arkit/arworldmap
- [A10] initialWorldMap — https://developer.apple.com/documentation/arkit/arworldtrackingconfiguration/initialworldmap
- [A11] getCurrentWorldMap(completionHandler:) — https://developer.apple.com/documentation/arkit/arsession/getcurrentworldmap(completionhandler:)

**Apple — CloudKit / DeviceCheck / Core Location / その他**

- [C1] CKContainer.publicCloudDatabase — https://developer.apple.com/documentation/cloudkit/ckcontainer/publicclouddatabase
- [C2] Designing and creating a CloudKit database — https://developer.apple.com/documentation/cloudkit/designing-and-creating-a-cloudkit-database
- [C3] CKQuery（位置述語、インデックス解像度） — https://developer.apple.com/documentation/cloudkit/ckquery
- [D1] DCDevice — https://developer.apple.com/documentation/devicecheck/dcdevice
- [D2] Establishing your app’s integrity（App Attest） — https://developer.apple.com/documentation/devicecheck/establishing-your-app-s-integrity
- [L1] Monitoring the user’s proximity to geographic regions — https://developer.apple.com/documentation/corelocation/monitoring-the-user-s-proximity-to-geographic-regions
- [L2] CLLocationManager.accuracyAuthorization — https://developer.apple.com/documentation/corelocation/cllocationmanager/accuracyauthorization
- [P1] Registering your app with APNs — https://developer.apple.com/documentation/usernotifications/registering-your-app-with-apns
- [S1] Sensitive Content Analysis — https://developer.apple.com/documentation/sensitivecontentanalysis
- [K1] In-App Purchase（StoreKit） — https://developer.apple.com/documentation/storekit/in-app-purchase
- [K2] PKCanvasView — https://developer.apple.com/documentation/pencilkit/pkcanvasview
- [K3] PhotosPicker — https://developer.apple.com/documentation/photosui/photospicker

**Apple — App Store Review Guidelines** — https://developer.apple.com/app-store/review/guidelines/

- [R1] 1.2 User-Generated Content
- [R2] 4.8 Login Services
- [R3] 5.1.1(v) Account Sign-In
- [R4] 3.1.1 In-App Purchase（抽選型の確率表示）

**Google — ARCore**

- [G1] Geospatial API overview — https://developers.google.com/ar/develop/geospatial
- [G2] Check VPS availability (iOS) — https://developers.google.com/ar/develop/ios/geospatial/check-vps-availability
- [G3] Authorization (iOS) — https://developers.google.com/ar/develop/authorization
- [G4] User privacy requirements — https://developers.google.com/ar/develop/privacy-requirements
- [G5] Cloud Anchors developer guide for iOS（TTL、クォータ） — https://developers.google.com/ar/develop/ios/cloud-anchors/developer-guide
- [G6] Cloud Anchors overview — https://developers.google.com/ar/develop/cloud-anchors

**Meta — Instagram**

- [M1] Sharing to Stories — https://developers.facebook.com/docs/instagram-platform/sharing-to-stories
