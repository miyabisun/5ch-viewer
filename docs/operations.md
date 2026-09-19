# 設定と画像キャッシュの保守

[README](../README.md)のCompose構成を前提に、必要な設定と既存データの保守手順を説明します。
コマンドは`compose.yaml`を保存したディレクトリで実行してください。

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
[Dockerfile](../Dockerfile)はコンテナ用の保存先を指定しています。
値は`PORT=3000`、`DATABASE_PATH=/data/5ch-viewer.db`、`IMAGE_CACHE_DIR=/data/images`です。READMEのCompose例では、
この`/data`全体を名前付きボリュームに保存します。Cookieも既定で`/data/cookies.json`に入ります。
実際のボリューム名はComposeのプロジェクト名を接頭辞に持ち、`docker compose config`で確認できます。
ホスト上のディレクトリへ保存する場合は、volumeの指定を`./data:/data`へ置き換えます。
DB・画像・Cookieをまとめて保護し、アプリ以外から書き込めない権限にしてください。

設定の実装: [src/config.rs](../src/config.rs)、[起動処理](../src/main.rs)、
[Cookie 保存](../src/fivech/cookie_jar.rs)、[SPA cache](../src/spa.rs)。
画像移行・リサイズ CLI も同じ設定を読み、`DATABASE_PATH` と `IMAGE_CACHE_DIR` を利用します。

### 開発・テスト用の設定

通常の配備では以下は不要です。

| 変数 | 必須・任意 | 未設定時 | 用途・不正値の扱い |
| --- | --- | --- | --- |
| `FIVECH_BASE_URL` | 任意 | サーバーごとの `https://{server}.5ch.io` | 5ch 取得先を mock などの単一 origin へ変更する。末尾 `/` を除去し、空なら通常の origin。URL 検証はなく、不正値は取得時に失敗する。release build でも有効。 |
| `FIVECH_ALLOW_LOOPBACK_FOR_TEST` | 任意 | loopback 許可なし | debug build で設定されていれば値によらず（空や `0` も）画像取得の loopback を許可する。release build では効果がなく、設定時に警告する。 |

`APP_PORT` / `MOCK_PORT`は統合テスト用binaryの設定です。
`VITE_API_TARGET`はVite開発proxyで使います。配布サーバーの待受設定は`PORT`です。

## 保守前の準備

通常の新規利用では、移行・一括縮小は不要です。既存データを処理する場合は、
そのボリュームを使うアプリを停止し、DB・画像・Cookieを含む`/data`全体を別の場所へバックアップしてください。
SQLiteのWALなどの付随ファイルも含めます。バックアップの中身を確認してから処理します。

```bash
docker compose stop viewer
```

以下の`run`はREADMEの`viewer`と同じデータボリュームを使います。
別の構成で運用している場合は、対象のDBと画像ディレクトリが一致することを確認してください。

## 画像キャッシュの移行

旧バージョンのSQLite BLOBキャッシュを利用している場合に、一度だけ実行します。
停止・バックアップ後、新バージョンのアプリを起動する前に行ってください。

```bash
docker compose run --rm --no-deps --entrypoint migrate-image-cache viewer
```

全画像を書き出して検証できた場合に限り、SQLiteの画像テーブルをメタデータ専用へ置き換えます。
再実行時はファイル監査のみを行い、`VACUUM`は実行しません。
`IMAGE_CACHE_DIR`はアプリ専用とし、アプリ実行ユーザー以外から書き込めない権限で管理します。

ソースから実行する場合は、リポジトリのルートで対象パスを指定します。

```bash
DATABASE_PATH=./data/5ch-viewer.db \
IMAGE_CACHE_DIR=./data/images \
cargo run --release --bin migrate-image-cache
```

## 既存画像キャッシュの一括縮小

停止・バックアップ後、まず`--dry-run`で対象とエラーを確認します。
問題がなければ本実行してください。

```bash
docker compose run --rm --no-deps --entrypoint resize-image-cache viewer --dry-run
docker compose run --rm --no-deps --entrypoint resize-image-cache viewer
```

対応する静止画像は、縦横比を保ったまま1024×1024ピクセルの枠内へ縮小します。
ドライランはファイルとDBを更新しません。本実行は再実行可能です。
一部の行で失敗した場合は非0で終了するため、表示された診断を修正してから再実行します。

ソースから実行する場合も、先にドライランを行います。

```bash
DATABASE_PATH=./data/5ch-viewer.db \
IMAGE_CACHE_DIR=./data/images \
cargo run --release --bin resize-image-cache -- --dry-run

DATABASE_PATH=./data/5ch-viewer.db \
IMAGE_CACHE_DIR=./data/images \
cargo run --release --bin resize-image-cache
```

処理が成功したら`docker compose up -d`でアプリを再開します。
復旧が必要な場合は停止状態を保ち、DBと画像を同じバックアップから戻してください。
縮小前の画質へ戻すには、処理前の画像が必要です。
