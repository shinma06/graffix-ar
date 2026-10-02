# 取り込み元と適合範囲

確認日: 2026-10-02。取得した固定 revision を比較して取り込み、元 repository・個人設定・実行状態は変更していません。

- [agent-harness-template / e822318](https://github.com/shinma06/agent-harness-template/tree/e822318a6c0fa7175a89687b929197dfae879184)
- [cursor-in-android-studio / a1e841c](https://github.com/shinma06/cursor-in-android-studio/tree/a1e841ccaeb113687d676536c2d13ae98fe77ef4)

| 管理能力 | 取り込み・適合先 |
|---|---|
| 共通 AGENTS、Claude/Cursor 入口、start/finish Skills | template を iOS と本 repository の branch 戦略へ適合 |
| Git branch/push guard、既存 hooks 保全、doctor/check | template。新規未追跡ファイルも検査し、実アプリ build へ接続 |
| GUI lease、private handoff registry | template の共通 namespace・厳格な private directory 検査を採用 |
| Issue 命名と type/priority/status | reference の schema・PR policy・回帰テスト |
| Project / Milestone / native Relationship | reference の責務・5 Views・終了照合を本アプリ向けに記述。ID は移植しない |
| develop/main、独立レビュー、受入 gate | reference の engine・JSON schema・trusted base 検証と回帰テスト |
| 自動 review/fix/publish/merge、停止/復旧、QA/cleanup | reference の coordinator・worker・QA handoff・source relocation と回帰テスト |
| 実サーバー保護 | 4 checks・strict base・会話解決・bypass なしの設定案。merge 時の実適用検査を追加 |
| 変更影響判定 | reference を Swift/Xcode/アプリ資産へ適合。未知・rename/delete/mode/不完全履歴は安全側 |
| 固定成果物 | plugin ZIP/JDK/IDE 検証を iOS .app ZIP、bundle/build、実行ファイル hash へ置換 |
| 既存 develop の履歴 | trusted main の明示 baseline と全 Case の実受入で移行可能。旧結果は継承しない |
| Governance audit | reference の読取専用監査契機と回帰テスト |

テンプレート単独では含まれなかった専用 coordinator を、利用者の指定により追加で移植しています。過去 Issue/Project/ruleset ID、製品 Case・レビュー合格・GUI 合格、既存 enrollment、scheduler/heartbeat、認証・trust・個人 config は移植していません。

IntelliJ 用の plugin 配布・JDK 互換性試験・Swing fixture は対象外です。iOS の署名・実機観察・ストア公開は別の実行環境と権限が必要です。管理用の Python は標準ライブラリのみで、新しい第三者依存を導入していません。

更新はこの固定 revision から差分を比較し、専用 Issue/PR で必要部分を取り込みます。元の成果物や実行状態を一括同期しません。適用済みと実運用済みの区別は [検証記録](validation.md)を参照してください。
