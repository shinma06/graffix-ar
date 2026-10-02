# ハーネスの更新と解除

導入元の固定 revision と適合範囲は [棚卸し](inventory.md)、初回 GitHub 適用は [セットアップ](setup/github.md) が正本です。通常の更新は専用 Issue/branch/worktree/PR で行います。

1. 導入元の更新差分を読み、この iOS 構成で必要な変更を選ぶ。blind copy・全体の再生成はしない。
2. AGENTS と同名 Skills、hooks、CI、受入 gate の既存契約を保つ。CLI の起動引数・GitHub API・Xcode build が変わる場合だけ現在の仕様を確認する。
3. `python3 scripts/check.py` と影響範囲の実アプリ確認を行う。check は追跡済みと非 ignore の新規ファイルを対象とする。
4. 別セッションの固定 HEAD/base レビューと、実 CI を確認して通常 PR で統合する。source SHA と変更理由を Issue/PR に残す。

`.agents/skills` を正本とし、Claude は symlink、Cursor は共通入口を維持します。実行中 claim/registry/GUI lease、認証・trust、個人設定を更新のついでに置換しません。元環境の合格結果・Project/ruleset ID を引き継ぎません。

解除も専用 PR として扱います。進行中の owner・worker・GUI・未保存成果物を確認し、`core.hooksPath` は導入前の値へこの repository だけ戻します。導入前に未設定だった場合だけ local の設定を解除します。既存 custom hooks、GitHub 保護、実行状態を一括削除せず、サーバー設定の変更は明示された範囲で個別に行います。
