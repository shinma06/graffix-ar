# 導入検証

確認日: 2026-10-02。元環境の成功をこの repository の成功へ転記しません。`pass / pending / fail / blocked / not-required` を分けます。

| 対象 | 状態・確認方法 |
|---|---|
| Python 管理テスト | pass。`python3 scripts/check.py`、template 由来 6 件＋管理 engine/iOS 適合 160 件、計 166 件 |
| アプリ Debug build | pass。Xcode 27.0 / 27A266a、iOS Simulator 27.0 SDK、共有 scheme Mock Up、arm64/x86_64、署名なしで実ビルド |
| Info.plist の移植性 | 旧個人絶対パスを `Graffix-AR/App/Info.plist` に変更し、上記 build で確認 |
| XCTest | not-required（今回アプリロジック変更なし）。現状テストソース・Testables がなく未実行 |
| AR 実機受入 | pending。カメラ・距離精度・ARSession の実機観察は未実施 |
| worker 起動方式 | 既存の ChatGPT 認証を確認。インストール済み CLI help と公式仕様で使用引数を確認。回帰試験は fake worker/隔離 process を使用 |
| 実モデルによる review/fix | 許可済みの別セッションで導入・同期をレビューし、2件を修正後に再承認。CLI worker と coordinator の実運用結果は [検証 Issue #5](https://github.com/shinma06/graffix-ar/issues/5) に記録。実モデルの修正 worker は未実施で、修正・再レビュー経路は回帰試験で確認 |
| 共通 entrypoints | AGENTS/Claude symlink/Cursor rule と start/finish Skills。各クライアントの新規セッション読込は pending |
| ローカル Git hooks | pass。既存 custom hooks がないことを確認し、`scripts/bootstrap.py` で `.githooks` を有効化 |
| GitHub の統合 | pass。[main 導入 PR #3](https://github.com/shinma06/graffix-ar/pull/3) と [既存 develop 同期 PR #4](https://github.com/shinma06/graffix-ar/pull/4) を4チェック成功後に統合。既存 develop の製品コード・履歴を保持 |
| GitHub の作業管理 | pass。[Project #4](https://github.com/users/shinma06/projects/4) の5 Views・Status・Relationship Status、[Milestone #1](https://github.com/shinma06/graffix-ar/milestone/1)、必要15ラベルを設定。Issue #2/#5 の登録・親子関係・両端の表示を readback |
| GitHub の保護 | pass。[main](https://github.com/shinma06/graffix-ar/rules/24365464) / [develop](https://github.com/shinma06/graffix-ar/rules/24365466) で PR 必須、strict base、test / PR policy / Acceptance gate / Agent review、会話解決、削除/force禁止、bypassなしを実効 API で readback |
| coordinator 接続 | pass。統合済み trusted main から `agent_loop.py scan` が実 GitHub 認証で成功。未登録 PR を処理しないことを確認 |
| scheduler | not-required。定期実行の登録・既存ジョブの再開はしていない |

## 再実行

```bash
python3 scripts/check.py
python3 scripts/doctor.py
python3 scripts/workflow/ios_build.py
git diff --check
```

check は tracked と非 ignore の新規ファイルについて、限定された設定構文・内部リンク・symlink・既知 secret 形式・個人 path・差分空白を確認し、隔離された Git/process/JSON fixture で回帰試験します。全 secret の検出や外部リンクの意味までは保証しません。CI、PR の独立 review、署名・実機観察は別の確認です。

Xcode がインストールされていても、現在の CLI 選択が CommandLineTools の場合は doctor が `xcode_selected: false` と表示します。確認時は実インストールの `DEVELOPER_DIR` をコマンド単位で指定し、マシンの既定設定は変更していません。

実務の所要時間・費用・token 削減は未測定です。
