# 5ch-viewer (goch-viewer)

スマホ・タブレット・PC のどこからでも、5ch スレッドの続きを同じ既読位置で読む
個人用ビューワー。dat と既読位置をサーバー（SQLite）に集約して端末間同期する。

- 仕様: [docs/spec.md](docs/spec.md)
- 実装メモ・判断ポイント: [docs/implementation-notes.md](docs/implementation-notes.md)

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
docker build -t goch-viewer .
docker run -p 3000:3000 -v goch-data:/data goch-viewer
```

## 環境変数

| 変数 | 既定 | 説明 |
| --- | --- | --- |
| `PORT` | 3000 | 待受ポート |
| `DATABASE_PATH` | ./data/5ch-viewer.db | SQLite パス |
| `BASE_PATH` | （空） | リバースプロキシのサブパス（例: `/5ch`） |
