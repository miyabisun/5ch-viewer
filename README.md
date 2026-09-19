# viewer-of-5ch

5chスレッドの本文と既読位置をサーバーに保存する、個人用のWebビューワーです。
スマホ・タブレット・PCから同じサーバーへアクセスし、続きを読むことができます。

- スレッドURLまたはタイトル検索からお気に入りを登録
- 既読位置の保存、未読数の表示、スレッドの更新
- 画像のキャッシュと、NG ID・NG Wordなどによる表示の絞り込み

一人で使うことを前提としており、アプリ内のログイン機能はありません。
別端末や外出先から使う場合は、Cloudflare Accessなど認証するプロキシを前段に置いてください。

## 起動する

DockerとDocker Composeを用意し、任意の作業ディレクトリへ次の`compose.yaml`を保存します。

```yaml
services:
  viewer:
    image: ghcr.io/miyabisun/5ch-viewer:latest
    ports:
      - "127.0.0.1:3000:3000"
    volumes:
      - viewer-of-5ch-data:/data
volumes:
  viewer-of-5ch-data:
```

そのディレクトリで起動します。

```bash
docker compose up -d
```

ブラウザで<http://localhost:3000>を開いてください。
この例は起動したPCからだけアクセスできます。別端末で使うには、前述の認証付きプロキシから接続します。
起動状況は`docker compose logs viewer`で確認し、停止には`docker compose stop viewer`を使います。

`/data`にはSQLiteのDB、画像、投稿用Cookieが保存されます。
名前付きボリュームに置くため、コンテナを作り直しても残ります。
`docker compose down -v`はこのボリュームも削除するので、データを残す場合は使わないでください。
保存場所の変更やサブパスでの公開は[設定と保守](docs/operations.md)を参照してください。

## スレッドを読む

1. 「スレッド登録」を開き、「URL指定」で5chのスレッドURLを貼り付けて「追加」を押します。
   「スレタイ検索」では、検索結果からスレッドを追加できます。
2. 「お気に入り」からスレッドを開きます。読み進めた位置は自動でサーバーへ保存されます。
3. 新着の本文を読みたいときは、スレッド内の「更新」ボタンを押します。
4. 別端末で同じサーバーのスレッドを開くと、保存された既読位置から再開できます。

端末を切り替える前に、通信できる状態でスクロールを止めて数秒待ってください。
既読位置はスクロール停止の約2秒後や画面を離れる際に送信します。通信失敗時の保存は保証できません。
スレッドの取得・検索には外部サービスへの接続が必要です。

## 更新・画像の保守

更新前に停止し、データをバックアップしてください。
旧SQLite BLOB形式の画像キャッシュがある場合は、[移行手順](docs/operations.md#画像キャッシュの移行)を先に確認します。
通常の更新は次のとおりです。

```bash
docker compose pull
docker compose up -d
```

保存済み画像の縮小は[一括縮小の手順](docs/operations.md#既存画像キャッシュの一括縮小)を参照してください。
停止・バックアップ・ドライランの順で確認します。

## 開発する

RustとBunを用意し、ソースを取得します。Dockerを使わず起動する例です。

```bash
git clone https://github.com/miyabisun/5ch-viewer.git
cd 5ch-viewer
cd client && bun install --frozen-lockfile && bun run build && cd ..
cargo run --release --bin viewer-of-5ch
```

Rust側のビルドには、LinuxではOpenSSLの開発ライブラリとpkg-configも必要です。
使用するツールの版と依存は[Dockerfile](Dockerfile)にあります。
既定の保存先は`./data/5ch-viewer.db`です。設定例は[.env.example](.env.example)を参照してください。
Node.js・npm・Bashも使える環境では、`bin/dev`でフロントを監視ビルドしながら開発できます。
テスト方法は[テスト構成](docs/testing.md)、実装の設計資料は[仕様書](docs/spec.md)にあります。

配布イメージは、Cargoの版と一致する`v*.*.*`タグからCIで作ります。
[release workflow](.github/workflows/release.yml)でAPI・SPA・実行ファイルの確認後にGHCRへ公開します。
