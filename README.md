# viewer-of-5ch

スマホ・タブレット・PC のどこからでも、5ch スレッドの続きを同じ既読位置で読む
個人用ビューワー。dat と既読位置をサーバー（SQLite）に集約して端末間同期する。

- 本アプリの仕様: [docs/spec.md](docs/spec.md)
- 5ch 側の仕様（調査メモ）: [docs/5ch-spec.md](docs/5ch-spec.md)

## 構成

- **バックエンド**: Rust（axum + tokio + rusqlite + reqwest + encoding_rs）。
  1 バイナリで SPA 配信 + API + バックグラウンド監視。
- **フロント**: Svelte 5 + Vite。
- **認証**: 前段の Cloudflare Access に委譲（アプリは認証を持たない）。

## 開発

```bash
bin/dev   # フロント watch build + cargo build + 起動（http://localhost:3000）
```

## 本番ビルド

```bash
cd client && bun install && bun run build && cd ..
cargo run --release
```

## Docker

```bash
docker build -t viewer-of-5ch .
docker run -p 3000:3000 -v viewer-of-5ch-data:/data viewer-of-5ch
```

## 環境変数

| 変数 | 既定 | 説明 |
| --- | --- | --- |
| `PORT` | 3000 | 待受ポート |
| `DATABASE_PATH` | ./data/5ch-viewer.db | SQLite パス |
| `IMAGE_CACHE_DIR` | DATABASE_PATH の親/images | 画像キャッシュファイルの保存先 |
| `BASE_PATH` | （空） | リバースプロキシのサブパス（例: `/5ch`） |

## 画像キャッシュの移行

旧バージョンのSQLite BLOBキャッシュを利用している場合は、アプリを停止してDBをバックアップした後、新バージョンを起動する前に一度だけ実行する。

```bash
DATABASE_PATH=./data/5ch-viewer.db \
IMAGE_CACHE_DIR=./data/images \
cargo run --release --bin migrate-image-cache
```

全画像をファイルへ書き出して検証できた場合に限り、SQLiteの画像テーブルをメタデータ専用スキーマへ置き換える。再実行時はファイル監査のみを行い、`VACUUM`は実行しない。

`IMAGE_CACHE_DIR`はアプリ専用とし、アプリ実行ユーザー以外から書き込めない権限で管理する。
