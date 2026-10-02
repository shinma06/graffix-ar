# 開発への参加

[プロジェクト情報](docs/project.md) と [開発手順](docs/workflow.md) を確認してください。作業は実在する Issue に紐づく `codex/<番号>-<内容>` などの専用 branch/worktree で進めます。

```bash
python3 scripts/bootstrap.py
python3 scripts/check.py
python3 scripts/workflow/ios_build.py
```

文書・管理ツールの確認と、アプリのビルド・実機受入を区別します。PR には [.github/pull_request_template.md](.github/pull_request_template.md) の metadata と実際の検証結果を記載します。Issue の分類・Project・Milestone は [作業管理](docs/work-management.md) に従います。
