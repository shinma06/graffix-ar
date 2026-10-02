# 受入・固定候補・実機 QA

`changes/issue-N.json` は Issue の受入、`promotion.json` は main に入れる固定候補と全 Case の実観察の正本です。過去の観察を別 SHA/build の pass に流用しません。未実施は pending、環境不足は blocked、製品不具合は fail と区別します。

## Issue の Case

次の例の番号・内容を実際の Issue に置き換え、`changes/issue-N.json` に保存します。GUI 不要なら具体的な理由・CLI 検証を残し、`gui_required: false` / `cases: []` にします。

```json
{
  "schema": 1,
  "issue": 12,
  "gui_required": true,
  "reason": "壁面への距離表示を変更するため実機確認が必要",
  "cli_checks": ["python3 scripts/workflow/ios_build.py: pass"],
  "cases": [{
    "id": "AR-DISTANCE-1",
    "artifact": "app",
    "change": "壁面距離の表示",
    "preconditions": "AR対応実機、カメラ許可、固定候補の署名済みアプリ、距離が既知の壁",
    "steps": ["壁を検出する", "合意した複数の距離で表示と実測を比較する", "バックグラウンドから復帰する"],
    "expected": "Issueで合意した誤差内で更新され、復帰後に検出が再開する",
    "provenance": ["Issue #12 の受入条件"],
    "gpt": {"status": "pending", "reason": "実機で未実施"},
    "human": {"status": "pending", "reason": "実機で未実施"},
    "fix_issue": null,
    "fix_pr": null,
    "recheck": "失敗は専用修正Issueへ引き継ぎ、新候補で再確認",
    "next_action": "担当者が固定buildを実機へ導入して結果を記録する"
  }]
}
```

`gpt` は参照元との schema 互換キーで、特定モデルを指定しません。人間または許可された実観察が有効です。Computer Use 経路そのものが要件の場合だけ `required_execution: computer_use` を指定します。fail には元 Issue と異なる実在 open な `fix_issue` が必要です。

カメラ拒否・ARSession 中断/復帰・距離精度・壁選択/色・長時間使用など、変更に必要な Case を漏れなく選びます。初期状態で全アプリのテスト結果を作ったり、固定の精度を発明したりしません。

## 固定 build

候補の clean な checkout で実行します。出力先は checkout 外の新規ディレクトリを指定します。

```bash
python3 scripts/workflow/release_candidate.py build \
  --source FULL_CANDIDATE_SHA --directory /tmp/graffix-ar-candidate --platform device --signed
python3 scripts/workflow/release_candidate.py check \
  --directory /tmp/graffix-ar-candidate --sha256 ACTUAL_ARTIFACT_SHA256
```

署名は既存の Xcode 設定を使います。build は source SHA・platform・署名指定・bundle/version/build・Xcode 版・配布 ZIP と実行ファイルの SHA-256 を manifest に記録します。Simulator/署名なし build はコンパイル確認用で、AR 実機受入や配布可能 IPA とみなしません。TestFlight/App Store 公開・証明書の設定はこの自動 merge には含みません。

## main promotion

1. 最新 main を develop へ専用同期 PR で取り込み、候補 SHA を固定する。
2. main にない候補の全 commit を、merge 済み develop PR に一意に対応付ける。各 PR の固定 merge SHA から Case JSON を読む。
3. 同じ候補・同じ app の全必要 Case を実施し、`promotion.json` に `schema: 1`、`base`（現在の main SHA）、`candidate`、`changes`（`commit` と `pr`）、`artifact_sha256`、`results` を保存する。
4. results のキーは `Issue番号:Case ID`。pass は `status / actor / observer / at / head / artifact_sha256 / evidence / loaded_identity / reason` を持つ。at は timezone 付き ISO8601、head/hash は候補と完全一致させる。loaded_identity には実際に導入・起動したアプリの bundle/build/実行ファイル hash 等の照合を書く。
5. 候補後に変更できるのは promotion.json と promotion Issue の Case JSON のみ。全範囲の gate・CI・独立レビューを通し、merge commit で main に入れる。一部 Case の pass で他の commit を混入させない。

生成した一覧は表示用です。JSON を更新して再生成し、二重の手入力台帳にしません。

```bash
python3 scripts/workflow/verification.py docs/verification/changes/issue-12.json \
  --promotion docs/verification/promotion.json --output docs/verification/current.md
```

## 導入前 develop 履歴の受入移行

導入時の develop には `185d716da909423b9d9d8c7ce5da9885b8a4d6ce` と `4cdc460372a068ce3349badaf0068d960907998c` が main より先行しています。履歴・旧 PR を書き換えません。

初回製品 promotion の前に実在の移行/QA Issue を作り、この固定 commit 集合と必要な実機 Case を `docs/verification/legacy-baseline.json` として **main 側の別レビュー済み PR** に保存します。形式は `schema: 1`、正の `issue`、全 SHA の `commits` 配列、上記と同じ形式の `acceptance`（gui_required=true、Case 必須）です。

promotion の changes では、この集合に限り `{"commit":"実SHA","baseline":true}` を使えます。gate は trusted main の計画、全 commit の一致、全 Case の候補/build 一致を要求します。旧履歴を検証不要とする例外ではありません。計画がない・commit 不足・Case 未実施なら停止します。現時点の観察や移行 Issue 番号を推測した baseline は同梱しません。

main 起点で限られた変更を検証する必要がある場合は、参照元同様に trusted main の `docs/verification/scopes/issue-N.json` へ許可ファイル・mode・受入を先に保存し、`scope: main` の promotion を使えます。通常の develop 全体の gate を暗黙に置き換えません。
