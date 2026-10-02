# GitHub の有効化

対象は `shinma06/graffix-ar`。ファイルの取り込みと GitHub サーバー設定の適用を区別します。操作前に既存 Issues/PRs、Project、ラベル、Milestone、rulesets を取得し、既存の所有・状態を保全します。

## 初回導入の順序

1. 公開・Issue/PR 作成の依頼範囲を確認し、[ラベル定義](../../.github/labels.json)を既存ラベルへ不足分だけ追加する。
2. 導入 Issue を作成し、具体的な受入・Project・Milestone・関係判定・claim を記録する。番号を branch と PR metadata に使う。
3. `codex/N-adopt-harness` の専用 worktree/branch で導入差分を PR にする。管理テスト、実 iOS build、別セッションの固定 HEAD/base レビューを行う。
4. 最初の main は validator 導入前の `7b55e8bff1c4583d2c1c68e7e7360e82a543ccf9`。この SHA・main target・`codex/N-adopt-harness` の組合せだけ初回 gate の bootstrap とし、通常のアプリ受入合格とは扱わない。PM が独立レビューに基づいて `Agent review` を記録し、残る checks を確認して通常 PR で統合する。
5. **既存 develop を作り直さない。** `codex/N-sync-harness` の専用 branch で main を既存 develop へ merge し、差分と検証を別 PR で確認する。旧 develop SHA `4cdc460372a068ce3349badaf0068d960907998c` とこの target/branch の組合せだけ同様の bootstrap を許可する。どちらも base が進めば使えない。
6. 両 target で実際に成功した `test` / `PR policy` / `Acceptance gate` / `Agent review` を確認してから、保護設定を適用・readback する。
7. Project 登録・Status・関係・Milestone を確認し、小さい PR を登録して自動進行の実認証・レビュー・修正・merge・QA・cleanup を一度通す。ローカル回帰テストはこの実運用試験の代替ではない。

初回 main PR の metadata は `Integration: tooling` とし、上記 SHA 限定の導入例外として Xcode の Info.plist パス修正を本文に明示し、実 build で検証します。通常の tooling PR は製品/Xcode 設定を通しません。bootstrap に旧製品の GUI pass や独立レビュー免除は含みません。

## Project・Milestone

[Project 設計](../../.github/project.json)に従って関連付け済み/同名 Project を再利用し、なければ **Graffix-AR 開発マップ** を private で作成・repository に関連付けます。公開範囲の変更は別の依頼がある場合だけ行います。

Now / Next / Later / Past / 全体の Views と、Relationship Status の 2 値を設定します。具体的な定義と Status 対応は [作業管理](../work-management.md)が正本です。Milestone は今回の到達目標「開発ハーネス運用開始」を候補に、同じ到達条件の既存目標を優先します。期日やリリース番号は推測しません。

Project API には適切な `read:project` / `project` 権限が必要です。アカウントの権限を黙って拡張せず、不足を具体的に記録します。[Projects API](https://docs.github.com/en/issues/planning-and-tracking-with-projects/automating-your-project/using-the-api-to-manage-projects)

## 保護設定

[main の設定案](../../.github/main-ruleset.json)と [develop の設定案](../../.github/develop-ruleset.json)は active、bypass なし、PR 必須、strict base、4 checks、会話解決、削除/force 禁止です。main は squash/merge、develop は squash。同一アカウント運用のため GitHub approval 数は 0 とし、固定 revision の別セッションレビューを `Agent review` で強制します。

既存同名 ruleset があればその ID を取得して更新します。存在しない場合の作成例:

```bash
gh api repos/shinma06/graffix-ar/rulesets --method POST --input .github/main-ruleset.json
gh api repos/shinma06/graffix-ar/rulesets --method POST --input .github/develop-ruleset.json
gh api repos/shinma06/graffix-ar/rulesets
gh api repos/shinma06/graffix-ar/rules/branches/main
gh api repos/shinma06/graffix-ar/rules/branches/develop
```

この例を未確認で繰り返して重複作成しません。GitHub プラン・権限・Actions 実行可否と [Rulesets の仕様](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/about-rulesets)を確認し、未反映を保護済みと記録しません。

## CI の境界

PR policy は checkout せず metadata をデータとして検査します。Acceptance gate は最新の trusted base を checkout し、PR JSON を `git show` で読みます。PR のアプリ/テストを実行する CI は read-only token、credential 非保持です。Agent review は coordinator が独立レビューと受入を照合して発行し、PR 内コードから自己承認しません。

既存 develop の導入前 commit には Case JSON がありません。初回の製品 promotion は [履歴の受入移行](../verification/README.md)を先に行い、未検証を自動で通す例外を追加しません。
