# 自動レビュー・修正・統合

`scripts/workflow/agent_loop.py` は、明示登録した同一 repository・maintainer の PR を一工程ずつ進めます。参照元の engine を移植し、Xcode build、共通 GUI lease、private registry に接続しています。スケジューラーへの登録と、実 PR の自動処理開始は別です。

## 起動条件

- [GitHub の有効化](github.md)を完了し、trusted main にこのハーネスを統合する。
- GitHub CLI の正規認証、Codex CLI の正規認証、Python 3.11 以上、必要時に Xcode を用意する。token・認証ファイルを複製しない。
- Issue の受入・ラベル・Project・Milestone・担当を確認し、専用 worktree の clean な HEAD と remote PR HEAD を一致させる。
- writer が編集・commit・push・GUI を停止し、scope と次の owner を確定する。
- worker 実行は実環境で許可される場合だけ使う。`codex exec` は coordinator が起動する子プロセスであり、名前を変えても独立 top-level セッションになるわけではない。子エージェント禁止の環境から `tick` を実行して迂回しない。許可された実行者へ packet を引き継ぐ。

## 登録と単発実行

以下は **trusted main checkout** で実行します。PR 側の coordinator を起動しません。値は実際の Issue/PR/worktree に置き換えます。

```bash
python3 scripts/workflow/agent_loop.py enroll \
  --pr 123 --source ../graffix-ar-issue-12 --owner implementation-session \
  --scope Graffix-AR/ docs/verification/ --writer-stopped
python3 scripts/workflow/agent_loop.py scan
python3 scripts/workflow/agent_loop.py tick --pr 123
```

`scan` は対象一覧を確認し、`tick` はレビュー、修正、再レビュー、CI 待ち、merge、終了処理のうち次の工程を実行します。結果を確認し、必要な次の tick を呼びます。PR を省略した tick は明示登録済みの最大 3 件を交代で処理します。`--parent` は実在する親がある場合だけ、`--close-issue` は main の全受入完了時だけ指定します。

公開 handoff は opaque ID・Issue/PR・HEAD/base/target・scope・停止宣言です。source/host は Git common-dir 配下の `agent-loop/registry` に 0600 で保存します。実行状態は移植元からコピーしません。

## 状態と制限

```text
queued → reviewing → reviewed
                     ├ changes_requested → fixing → publishing → reviewing
                     ├ blocked / ci-failed → 明示的な復旧待ち
                     ├ acceptance-wait / ci-wait → 条件待ち
                     └ approved → merging → cleanup → done
```

reviewer は read-only、fixer は指定 scope の workspace-write。GitHub token を環境から除き、個人 config の MCP/plugin 設定を読み込ませません。モデルは利用者の選択を維持します。worker は 10 分、fix は 3 回、review は 8 回、通信等の失敗は 3 回で停止します。終了時に worker の子プロセスも停止します。[Codex 非対話実行](https://learn.chatgpt.com/docs/non-interactive-mode)・[CLI 引数](https://learn.chatgpt.com/docs/developer-commands?surface=cli)に基づく起動です。

base 更新は通常 merge → 必要テスト → 非 force push → 独立再レビュー。HEAD/base/target/Issue/本文/feedback が変わると旧承認を失効させます。4 checks、会話解決、strict base、force/削除禁止、bypass なしの実サーバー設定を確認できなければ merge しません。CI を待つ間の Agent review は独立レビューの記録であり、GUI 受入を作り出しません。

## 復旧

```bash
python3 scripts/workflow/agent_loop.py resume --pr 123 --reason '停止原因を解消しworker停止と所有を確認'
python3 scripts/workflow/agent_loop.py rebind-target --pr 123 --writer-stopped
python3 scripts/workflow/agent_loop.py cleanup-branches
# 候補の所有・clean・停止・remote ref を確認した場合だけ --apply を付ける
```

resume は停止原因と元 writer/worker の停止を確認してから使います。target 変更は rebind-target で明示し、旧承認を再利用しません。source が消失した場合は旧所有の確認と private 移転記録が必要です。registry 喪失、別 host、外部 push、dirty を reset/stash で隠しません。

develop の実装完了は QA Issue を作成/再利用し、Case・merge SHA・Milestone・native sub-issue・双方向リンクの readback 後に close します。Project 表示の同期は PM が [終了確認](../work-management.md) で行います。cleanup は自分の停止済み clean な資源だけが対象で、merge 成功と cleanup 完了は分けます。

## 定期実行

定期実行は明示依頼がある場合に利用環境の正規スケジューラーへ登録します。[coordinator 依頼文](../../prompts/coordinator.md)を使い、trusted directory、対象、worker 許可、時間・回数予算、停止・復旧条件を設定します。変更なしは通知せず、進展・完了・障害・利用者の操作が必要な場合だけ通知します。既存 PAUSED ジョブを再開しません。今回、cron・LaunchAgent・heartbeat は作成していません。
