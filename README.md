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
cd client && bun install --frozen-lockfile && bun run build && cd ..
cargo run --release
```

## Docker

```bash
docker build -t viewer-of-5ch .
docker run -p 3000:3000 -v viewer-of-5ch-data:/data viewer-of-5ch
```

`v*.*.*` タグでイメージを GHCR へ公開する。タグは `Cargo.toml` のバージョンと
一致させる。公開前に `.github/smoke-image.sh` で3実行ファイル・API・SPAを確認する。
依存ビルドは cargo-chef で分離し、GHCR の製品イメージとは別の `build-cache` タグに
Buildx の中間段階を保存する。ソースやバージョンだけの変更では依存を再利用する。

## 環境変数

起動時にプロセス環境と `.env` を読みます。既存のプロセス環境が優先です。
以下はアプリ単体の既定値で、すべて任意です。

| 変数 | 必須・任意 | 未設定時 | 用途・空値や不正値の扱い |
| --- | --- | --- | --- |
| `PORT` | 任意 | `3000` | 待受ポート。`u16` として解析できない値（空・負数・65535超過など）は既定値。`0` は OS が空きポートを選択。bind 失敗時は起動失敗。 |
| `DATABASE_PATH` | 任意 | `./data/5ch-viewer.db` | SQLite 保存先。親ディレクトリの作成を試み、DB を開けない場合は起動失敗。空値もそのまま SQLite に渡す。 |
| `IMAGE_CACHE_DIR` | 任意 | `DATABASE_PATH` の親ディレクトリ内の `images`（親がなければ `./images`） | 画像キャッシュ保存先。前後空白を除去し、空なら既定値。書込み不能なパスはキャッシュ保存時に失敗する。 |
| `COOKIES_PATH` | 任意 | `DATABASE_PATH` の親ディレクトリ内の `cookies.json`（親がなければ `./data/cookies.json`） | 投稿用 Cookie の永続化先。空もそのまま使用。ファイル読込み不能・JSON 不正時は空の Cookie jar で起動し、保存失敗時は警告して続行する。 |
| `BASE_PATH` | 任意 | 空 | リバースプロキシのサブパス（例: `/5ch`）。末尾 `/` を除去し、空や `/` はルート。その他は `^/[\w\-/]*$` に一致しなければ起動失敗。 |
| `NODE_ENV` | 任意 | 更新時刻に応じて SPA HTML を再読込 | 完全一致の `production` だけ HTML のキャッシュを更新時刻が変わっても再利用する。空・誤記を含む他の値は更新時刻に応じて再読込。認証設定ではない。 |
| `RUST_LOG` | 任意 | `info` | ログのレベル・target filter（例: `debug`、`viewer_of_5ch=debug`）。空は `error`。構文不正は標準エラーに警告し、ログを無効化する。`LOG_LEVEL` は参照しない。 |

待受アドレスは `0.0.0.0`（全 IPv4 インターフェース）固定です。旧 `BIND_ADDRESS` は
参照しません。ポート変更には `PORT` を使い、公開範囲は外側の配布設定で管理します。
コンテナ内の `PORT` を変える場合は、公開ポートの転送先も同じ値にします。
[Dockerfile](Dockerfile) は `PORT=3000`、`DATABASE_PATH=/data/5ch-viewer.db`、
`IMAGE_CACHE_DIR=/data/images` を明示します。host 上の保存先と mount は
[home-server の README](https://github.com/miyabisun/home-server/blob/main/README.md) と
[home/compose.yaml](https://github.com/miyabisun/home-server/blob/main/home/compose.yaml) を参照してください。

設定の実装: [src/config.rs](src/config.rs)、[起動処理](src/main.rs)、
[Cookie 保存](src/fivech/cookie_jar.rs)、[SPA cache](src/spa.rs)。
画像移行・リサイズ CLI も同じ設定を読み、`DATABASE_PATH` と `IMAGE_CACHE_DIR` を利用します。

### 開発・テスト用の設定

通常の配備では以下は不要です。

| 変数 | 必須・任意 | 未設定時 | 用途・不正値の扱い |
| --- | --- | --- | --- |
| `FIVECH_BASE_URL` | 任意 | サーバーごとの `https://{server}.5ch.io` | 5ch 取得先を mock などの単一 origin へ変更する。末尾 `/` を除去し、空なら通常の origin。URL 検証はなく、不正値は取得時に失敗する。release build でも有効。 |
| `FIVECH_ALLOW_LOOPBACK_FOR_TEST` | 任意 | loopback 許可なし | debug build で設定されていれば値によらず（空や `0` も）画像取得の loopback を許可する。release build では効果がなく、設定時に警告する。 |

統合テスト用 binary の `APP_PORT` / `MOCK_PORT`、Vite 開発 proxy の
`VITE_API_TARGET` は開発用の設定であり、配布するサーバーの待受設定ではありません。

## 画像キャッシュの移行

旧バージョンのSQLite BLOBキャッシュを利用している場合は、アプリを停止してDBをバックアップした後、新バージョンを起動する前に一度だけ実行する。

```bash
DATABASE_PATH=./data/5ch-viewer.db \
IMAGE_CACHE_DIR=./data/images \
cargo run --release --bin migrate-image-cache
```

全画像をファイルへ書き出して検証できた場合に限り、SQLiteの画像テーブルをメタデータ専用スキーマへ置き換える。再実行時はファイル監査のみを行い、`VACUUM`は実行しない。

`IMAGE_CACHE_DIR`はアプリ専用とし、アプリ実行ユーザー以外から書き込めない権限で管理する。

## 既存画像キャッシュの一括縮小

本番環境の既存キャッシュは、アプリを停止し、`DATABASE_PATH`のDBと`IMAGE_CACHE_DIR`をバックアップした後に処理する。まず`--dry-run`で対象とエラーを確認し、問題がなければ本実行する。

```bash
DATABASE_PATH=./data/5ch-viewer.db \
IMAGE_CACHE_DIR=./data/images \
cargo run --release --bin resize-image-cache -- --dry-run

DATABASE_PATH=./data/5ch-viewer.db \
IMAGE_CACHE_DIR=./data/images \
cargo run --release --bin resize-image-cache
```

Dockerイメージにも専用バイナリが含まれる。上記のDocker例と同じデータボリュームを使う場合は、そのボリュームを使用するアプリコンテナを停止してから実行する。

```bash
docker run --rm -v viewer-of-5ch-data:/data \
  --entrypoint resize-image-cache viewer-of-5ch --dry-run

docker run --rm -v viewer-of-5ch-data:/data \
  --entrypoint resize-image-cache viewer-of-5ch
```

対応する静止画像は縦横比を保ったまま1024×1024ピクセルの枠内に縮小される。ドライランはファイルとDBを更新しない。本実行は再実行可能で、一部の行で失敗した場合は非0で終了するため、表示された診断を修正後に再実行する。
