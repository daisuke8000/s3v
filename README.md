# s3v - S3 Viewer TUI

Read-only な S3 ブラウザ TUI アプリケーション（Rust 製）

## 機能 (Phase 1 MVP)

- S3 バケット・フォルダのブラウジング
- Vim スタイルナビゲーション (j/k または矢印キー)
- Esc で親フォルダに戻る
- 非同期 S3 API 操作

## インストール

```bash
cargo install --path .
```

## 使い方

```bash
# バケット一覧から開始
s3v

# 特定のバケットを開く
s3v my-bucket

# 特定のパスを開く
s3v my-bucket/path/to/folder

# S3 URI を指定
s3v s3://my-bucket/path

# AWS プロファイルを指定
s3v --profile myprofile

# カスタムエンドポイント (MinIO, LocalStack)
s3v --endpoint http://localhost:9000
```

## キーバインド

| キー | アクション |
|------|-----------|
| `↑` / `k` | 上に移動 |
| `↓` / `j` | 下に移動 |
| `Enter` | フォルダを開く |
| `Esc` | 戻る |
| `q` | 終了 |

## 必要条件

- AWS 認証情報の設定（環境変数、~/.aws/credentials、または IAM ロール）

## ライセンス

MIT
