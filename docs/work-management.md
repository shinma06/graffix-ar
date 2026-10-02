# Issue・Project・Milestone の管理

参照元と同じ責務分担で運用します。Issue は具体的な作業と受入、Project は全体の状態・優先順位・ロードマップ、Milestone は到達目標、native Relationship は実際の分解・依存を表します。

## Issue 作成・triage

1. 重複・コメント・関連 PR・既存 claim を確認し、単一の目的・対象外・観察可能な受入・次の操作を書く。
2. Project に登録する。未接続なら未反映の対象、担当、再試行条件を Issue に記録する。
3. 既存の適切な Milestone を選ぶ。未決なら理由・判断担当・判断条件を記載し、架空の版や期日を作らない。
4. 実際の分割なら parent/sub-issue、開始・完了に必要な先行作業なら blocked by/blocking を設定する。分類のための万能親や偽の依存を作らない。
5. native 関係の両端を読み戻す。関係なしは正常な `Standalone`、関係ありは `Has Relationship`。未判定は空欄とし、空欄を Standalone と推測しない。
6. 題名と各軸ちょうど 1 つのラベルを確認する。

| type ラベル | 題名 |
|---|---|
| `type:feature` | `[機能] 要約` |
| `type:bug` | `[修正] 要約` |
| `type:research` | `[調査] 要約` |
| `type:qa` | `[試験] #元Issue番号 要約` |
| `type:maintenance` | `[運用] 要約` |
| `type:tracking` | `[追跡] 要約` |

priority は `priority:P0` / `priority:P1` / `priority:P2`、status は `status:ready` / `status:in-progress` / `status:review` / `status:blocked` / `status:deferred` / `status:done`。closed は done、open は done 以外。優先度を題名に重複させません。PR policy が型・番号・ラベルを検査します。

## Project の表示

Project 名は **Graffix-AR 開発マップ** を使用します。同名・関連付け済み Project を先に確認して再利用し、参照元の Project #2 をこのアプリへ流用しません。設定案は [.github/project.json](../.github/project.json)。実 ID は GitHub で取得して記録します。

| View | filter | 役割 |
|---|---|---|
| Now — 進行中 | `is:open label:status:in-progress,status:review -label:type:tracking` | 現在の担当と PR |
| Next — 着手候補 | `is:open label:status:ready -label:type:tracking` | 次の候補 |
| Later — 依存待ち・保留 | `is:open label:status:blocked,status:deferred` | 再開条件 |
| Past — 完了履歴 | `is:closed` | 経緯・完了日 |
| 全体 — 親子と全Issue | filter なし | 分解・横断 QA |

Status は `ready/blocked/deferred → Todo`、`in-progress/review → In Progress`、`closed/done → Done`。Priority は Labels で表示し、同じ情報を別の手入力フィールドにしません。Milestone・Parent・Linked pull requests は標準フィールド、`Relationship Status` のみ 2 値の追加フィールドです。概要に過去・現在・次の節目・依存順を短く記載します。

## 中断・終了

Issue に owner、HEAD/base/target、PR、dirty の有無、検証、残条件、次の操作、claim の継続/解放を残します。時間経過や応答なしで担当を奪いません。worktree の絶対パス、host、認証値は private registry だけに保存します。

develop 統合後、coordinator は元 Issue の実装受入を照合し、残る GUI/main 確認を QA へ全件引き継ぎます。QA を実際の sub-issue とし、Milestone を継承し、双方向リンクを読み戻してから元実装 Issue を close します。QA・親・Milestone は子 PR の merge だけで完了にしません。

PM は元 Issue・QA の Project 登録と Status、Milestone、native 関係、両端の Relationship Status を読み戻します。Project の更新は coordinator が自動実行したとみなさず、未反映は担当・次操作を記録します。主要な管理変更、Milestone 完了、10 件の実 merge を目安に `python3 scripts/workflow/governance_audit.py` で監査契機を確認し、必要な範囲だけ見直します。
