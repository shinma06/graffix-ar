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
| 実モデルによる review/fix | pending。実 PR 登録・実認証・利用可能な実行者での試行は未実施 |
| 共通 entrypoints | AGENTS/Claude symlink/Cursor rule と start/finish Skills。各クライアントの新規セッション読込は pending |
| ローカル Git hooks | pass。既存 custom hooks がないことを確認し、`scripts/bootstrap.py` で `.githooks` を有効化 |
| GitHub | read-only 接続確認。main/develop/既存 feat branches を確認。導入開始時の active ruleset は 0 |
| GitHub 設定・PR の公開 | 進行中。[導入 Issue #2](https://github.com/shinma06/graffix-ar/issues/2)、[Project #4](https://github.com/users/shinma06/projects/4)、[Milestone #1](https://github.com/shinma06/graffix-ar/milestone/1) と必要ラベルを作成。PR・rulesets・CI は適用後に readback |
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
