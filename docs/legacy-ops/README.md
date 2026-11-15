# 旧運用の保管と戻し方

2026-11 のコンセプトからの再構築（`rebuild/concept-rust-core`）に合わせて、Codex・Claude・Cursor の複数エージェント運用（Issue/Project 管理、`develop` 統合、Case 台帳による受入ゲート、`scripts/workflow/*`、`.githooks`）から、Devin × Entire の運用へ切り替えました。旧運用のファイルは既存の Git 参照に残っています。

| 参照 | 内容 |
|---|---|
| `main`（`fbabc42`） | 再構築を取り込む前の `main`。旧運用一式（`AGENTS.md`、`docs/workflow.md` ほか、`scripts/workflow/*`、`.github/workflows/{acceptance,pr-policy}.yml`） |
| `develop`（`44af522`） | 切り替え直前の `develop` |

再構築 PR（#7）を `main` に取り込むと `main` は旧運用を指さなくなるため、取り込み前に上記 2 つの commit にタグを付けて固定してください（例: `legacy-ops-main` → `fbabc42`、`legacy-ops-develop` → `44af522`）。

## GitHub 側の設定

`main-ruleset.json` / `develop-ruleset.json` は旧運用のサーバ設定の写しです。2026-11-16 時点でサーバの ruleset `main-pr-required` / `develop-pr-required` はまだこの内容のままで、必須チェック `test` / `PR policy` / `Agent review` / `Acceptance gate` は現在の CI（`Rust core (Linux)` / `iOS app (Xcode)`）では出力されません。新運用に合わせた変更は所有者が行います。

## 戻し方

1. 旧運用のファイルを上記の参照から復元する（`git checkout fbabc42 -- <path>`）。
2. GitHub の ruleset を、このフォルダの `main-ruleset.json` と `develop-ruleset.json` の内容に戻す（必須チェック: `test`、`PR policy`、`Agent review`、`Acceptance gate`）。
3. 旧 `.githooks` を `scripts/bootstrap.py` で有効化する。
