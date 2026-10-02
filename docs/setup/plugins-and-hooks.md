# Plugins・通知・hooks

このハーネスの管理機能は Python 標準ライブラリ、Git、GitHub CLI、Codex CLI で動きます。追加の Plugins・通知アプリは必須ではありません。設定ファイルや trust hash を別の環境からコピーしません。

既存の Ponytail 等を使う場合は、利用クライアントの正規 installer と本人の trust 操作を使い、採用版と実イベントでの動作を確認します。manifest の存在は実行成功の証拠ではありません。子エージェント用の hook があっても、子エージェント起動の権限は増えません。

通知や terminal 連携が必要な場合だけ公式の配布経路で導入し、OS の通知権限・PATH・重複通知を確認します。個人の launcher、app 内部 helper の絶対パス、認証情報を repository に追加しません。MCP は [接続手順](mcp.md)、GUI は [共通 lease](../operations.md) に従います。

repository の Git hooks は `python3 scripts/bootstrap.py` で有効化します。既存 hooks を検出した場合は保全して統合し、無効化して回避しません。クライアント側の hook と Git hooks は別の設定です。導入・更新後は実際の最小操作で確認し、結果と未確認を [検証記録](../validation.md) に残します。
