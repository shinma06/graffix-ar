# 開発・レビュー・二段階統合

作業は Issue → claim → 専用 branch/worktree → Draft PR → 必要な検証 → 独立レビュー → target 別 gate → merge → QA/Project/後片付けの順に進めます。読み取りだけの相談・レビューでは Issue を新設しません。公開操作は利用者の依頼範囲に従います。

## 開始と所有

Issue の全コメント、関連 PR、依存、既存 claim、dirty、worktree を確認します。owner・対象/対象外・受入・base SHA・target・reviewer・GUI 要否・次操作を記録し読み戻します。一つの作業範囲の writer は一人です。

```bash
git status --short --branch
git worktree list
git fetch --prune origin
# 例: 実在する Issue 12 の製品変更。番号・slug は実際の作業へ変更する
git worktree add -b codex/12-wall-measurement ../graffix-ar-issue-12 origin/develop
cd ../graffix-ar-issue-12
git branch --unset-upstream
python3 scripts/bootstrap.py
```

branch は `codex|claude|cursor|agent/<Issue番号>-<slug>`。運用変更だけなら origin/main を起点にします。main/master/develop へ直接 commit/push、force push、hook/保護の迂回、他担当の変更破棄は禁止です。既存の `feat/*` は履歴として保持し、新規作業で流用しません。

## 検証と PR

要件・呼出し元/先・既存テストを読み、既存実装・標準機能から最小の変更を選びます。`python3 scripts/check.py` は管理ファイルと回帰テスト、アプリ変更は [実ビルド](project.md) を実行します。変更影響判定は hook/CI/coordinator で `scripts/workflow/change_impact.py` を共用し、rename/delete/mode 変更・未知・不完全履歴を安全側に扱います。

最初の意味ある push で Draft PR を作成し、PR テンプレートの `Issue / Integration / Verification / GUI / GUI reason` を埋めます。受入は `docs/verification/changes/issue-N.json` に記録します。独立したセッションへ固定 HEAD/base と受入を渡し、具体的な指摘を修正して必要な再レビューを行います。自己レビューや CI 成功を独立レビューの代わりにしません。

| target / Integration | 統合条件 | merge |
|---|---|---|
| develop / `develop` | 必要テスト、独立コードレビュー、全必要 Case と次操作。GUI pending/blocked/fail は保存し、fail は修正 Issue に紐づける | squash |
| main / `promotion` | 固定候補の全 commit/Case、同じ build の実観察、独立レビュー、必要 checks | merge commit |
| main / `tooling` | docs/scripts/CI/agent 入口のみ、GUI 不要の具体的理由と CLI 検証 | squash |

製品コード・Xcode 設定を tooling として迂回させません。develop は `Refs #N` を使い、自動 close 文言は禁止です。独立レビュー・CI 失敗は GUI 未実施とは別で、解消するまで統合しません。

## 自動進行と終了

writer が編集・commit・push・GUI を停止してから [自動進行](setup/automation.md) へ登録します。4 checks（test、PR policy、Acceptance gate、Agent review）と会話解決、現在の HEAD/base/受入を再確認し、保護付きの通常 PR merge を使います。

merge 後は [作業管理の終了確認](work-management.md) と QA 引継ぎを行います。remote 実 ref、local branch、remote-tracking ref、worktree を照合し、自分の停止済み・clean な資源だけを整理します。squash 後の `--merged` や `[gone]` だけを削除根拠にせず、PR HEAD と統合結果を確認します。main/master/develop、他担当、GUI 使用中、未公開成果物は保全します。

今回の初回導入・既存 develop への同期は [GitHub 有効化手順](setup/github.md) に従います。新しい規約の追加を、未依頼の公開・他セッションへの連絡・定期実行を始める権限として扱いません。
