# Graffix-AR

SwiftUI・ARKit・SceneKit による iOS の壁面検出・距離測定アプリです。実装の入口とビルド条件は [プロジェクト情報](docs/project.md) を参照してください。

```bash
python3 scripts/doctor.py
python3 scripts/check.py
python3 scripts/workflow/ios_build.py
```

Python 3.11 以上と、iOS SDK を含む Xcode が必要です。Xcode がコマンドラインで選択されていない場合は、実際の Xcode に `DEVELOPER_DIR` を設定してください。ビルド成功は AR 実機試験の合格を意味しません。

開発管理には [agent-harness-template](https://github.com/shinma06/agent-harness-template) と [cursor-in-android-studio](https://github.com/shinma06/cursor-in-android-studio) の管理・自動進行コードを取り込み、iOS 向けに適合しています。

- [開発・終了の手順](docs/workflow.md)：Issue、担当、専用 worktree、PR、二段階統合。
- [作業管理](docs/work-management.md)：Project、Milestone、親子・依存関係、終了時の整合。
- [自動レビュー・修正・統合](docs/setup/automation.md)：登録、単発進行、停止・再開、QA 引継ぎ。
- [受入・固定候補](docs/verification/README.md)：実機 Case、SHA・配布物の固定、main への反映。
- [GitHub の有効化](docs/setup/github.md)：ラベル、Project、4 つの必須チェックと保護設定。
- [取り込み範囲](docs/inventory.md)・[検証結果](docs/validation.md)：確認済みと、外部で有効化する項目。

`AGENTS.md` が共通の正本です。Claude Code と Cursor も同じ規約を参照します。ローカル hooks は内容を確認して `python3 scripts/bootstrap.py` で導入します。
