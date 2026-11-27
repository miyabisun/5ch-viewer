# テスト構成

このプロジェクトのテストは「層ごとに速さと網羅を分ける」方針。

| 層 | コマンド | 何を検証するか | バックエンド |
| --- | --- | --- | --- |
| Rust 単体 | `cargo test` | パーサ / DB / dat 保存・板同期対象のロジック | 実 DB(:memory:)・HTTP なし |
| フロント単体 | `cd client && bun run test` (vitest) | 純関数(name 整形 等)。`src/lib/*.test.js` | なし |
| 画面(jsdom) | `cd client && bun run test` (vitest + jsdom) | 文言・出し分け・メニュー・送る要求・状態の移り変わり。`src/page/*.test.ts` | 試験の中の偽の応答(`src/testing/server.ts`) |
| API 総合 | `cargo test --test backend` | 実ルーター → 実 DB → 5ch モックの HTTP の一気通貫 | itest-server(:memory: DB + 5ch モック) |
| 画面(実ブラウザ) | `cargo test --test page` | 配置・大きさ・スクロール・解決された色・端末ごとのスタイル・ブラウザの履歴と再読み込み、画面から実 DB までの一気通貫 | itest-server。形の決まったデータが要る試験はページ内の偽の応答 |
| 配布イメージ | `docker build -t viewer-of-5ch:test . && bash tests/smoke-image.sh viewer-of-5ch:test` | API・SPA・同梱実行ファイルの起動と依存 | 新規コンテナ(本番データなし) |

commit 前に手元で format・lint と上の各層を通す。CI は tag からイメージを作って公開するだけで、検査はしない。
`cargo test` は `tests/page.rs` も流す。これは `client/build` の画面を使うので、先に `bun run build` する。

```
cd client && bun run format:check && bun run test && bun run build && cd ..
cargo fmt --check && cargo clippy --locked --all-targets --all-features -- -D warnings && cargo test
```

「テスト緑 = 実機で直る」を担保するのが総合テスト。reload ゲート / dat 取得 / DB 全置換 /
お気に入り表示など、バックエンドが絡む回帰は API 総合と画面の一気通貫で実フロー検証する。

## 画面(実ブラウザ)の動かし方

`tests/page.rs`(`harness = false` の自前の runner)が、Chrome for Testing の
`chrome-headless-shell` と同じ版の `chromedriver` を 1 つずつ起こし、W3C WebDriver の HTTP を
直に呼んで試験を書いた順に 1 本ずつ流す。browser は `bin/browser`(python3 の標準 library だけ)が
版と SHA-256 を固定して `${XDG_CACHE_HOME:-~/.cache}/chrome-for-testing/<版>/` へ取る(在れば network に出ない)。
`bin/test_browser.py` がその取り方を確かめる。

- アプリは itest-server を OS が選ぶ空き port で起こす(`tests/support/itest.rs`)。画像キャッシュと cookie は一時 directory。
- 形の決まったデータ(ワッチョイ名・長い ID・多数のレス)が要る試験は、ページ内の `fetch` を `tests/support/page.js` の偽の応答に差し替える(`fake`)。
- `cargo test --test page -- <語>` で題に語を含む試験だけ、`--skip <語>` で除く。

## 5ch アクセスの差し替え

`src/fivech/http.rs` は通常 `https://{server}.5ch.io` にアクセスするが、
**環境変数 `FIVECH_BASE_URL`** を設定すると全リクエストをその origin に向ける。

- 本番: 未設定 → `https://{server}.5ch.io`(従来どおり)
- テスト: `FIVECH_BASE_URL=http://127.0.0.1:3002` → ローカルのモック 5ch へ

差し替えても SSRF 検証(`validate_ref`)・Monazilla UA・Shift_JIS デコード・
`Accept-Encoding: identity` はそのまま。変わるのはホスト部分のみ
(`/{board}/subject.txt`・`/{board}/dat/{thread_id}.dat`・`/{board}/SETTING.TXT` のパスは固定)。

## テストサーバー(itest-server)

`src/bin/itest-server.rs` が 1 プロセスで 2 つの HTTP サーバーを立てる。

- アプリ本体: `routes::build_router` をそのまま使い、**インメモリ SQLite**(`:memory:`、
  単一 Connection なのでプロセス生存中は保持)で起動。`fivech_base_url` をモックへ向ける。
  バックグラウンド同期(`start_sync`、180 秒間隔)は起動しない
  (総合テストの決定性を保つため。reload と板同期はテストが明示的に駆動する)。
- モック 5ch: `subject.txt` / dat / `SETTING.TXT` を返す。レス数や dat 消失(404)を
  実行時に差し替え可能。

ポートは既定で OS が選び、両方が待ち受けたら `[itest-server] app=http://… mock=http://…` の
1 行を出す。環境変数 `APP_PORT`・`MOCK_PORT` で指定もできる。`IMAGE_CACHE_DIR`・`COOKIES_PATH`
で画像キャッシュと cookie の置き場を変える(試験は一時 directory を渡す)。

```
cargo run --bin itest-server
```

本番とは別ポート・別 DB(:memory:)なので、本番データには一切触れない。

### 制御エンドポイント(テスト専用)

アプリ側(`APP_PORT`):

- `POST /_control/reset` — favorites を全削除(dat_blobs は FK CASCADE で連動削除)。
  favorites への FK を持たない own_posts・ng_ids・ng_words・ng_wacchoi も明示的に削除する
  (残すと NG ルールが次のテストへ漏れる)。各テスト前に呼ぶ。
- `POST /_control/seed-favorite`
  `{ server, board, thread_id, title, res_count, blob_posts }`
  favorite と dat_blobs を投入。`res_count`(メタ)と `blob_posts`(実 blob のレス数)を
  別々に指定でき、**ドリフト**(メタ 117 / blob 111 など)を再現できる。
- `POST /_control/refresh-board` `{ server, board }` — 板同期を非同期で起動する。
  `refresh_board` → `refresh_board_with_subject` を呼び、保存結果はテストからポーリングする。

モック側(`MOCK_PORT`):

- `POST /_control/thread`
  `{ server, board, thread_id, title, res_count, dat_posts, gone }`
  subject.txt が報告するレス数(`res_count`)と dat が返すレス数(`dat_posts`)を指定。
  `gone: true` で dat を 404 にしてスレ落ちを再現。
- `POST /_control/reset` — モックのスレ定義と subject ヒットカウンタを全消去。
- `GET /_control/subject-hits/{board}` — その板の subject.txt が叩かれた回数を返す。
  板同期が subject.txt を**板 1 回**に抑えることと、単一スレの reload では
  subject.txt を取得しないことを検証するカウンタ。

## 総合テストの動かし方

`tests/backend.rs` の各試験が itest-server を 1 つずつ空き port で起こし、HTTP で叩く
(試験ごとに別の :memory: DB なので並べて流れる)。`tests/page.rs` の一気通貫の試験
(`tests/page/stack.rs`)は、itest-server が配る `client/build` の画面を実ブラウザで操作し、
フロントは普段どおり `/api/...` を叩いて実 Rust に届く。

```
cargo test --test backend
cargo test --test page
```

### reload と板同期の取得条件

- **単一スレの reload**: フッターの更新ボタン・投稿成功後に
  `GET /api/favorites/{server}/{board}/{thread_id}/reload` を呼ぶ。
  dat への HEAD の `Content-Length` と保存済み `dat_bytes` (取得時の Shift_JIS バイト数)を
  比較し、保存サイズが正かつ一致すれば本文 GET を省略する。不一致・保存サイズ不明・
  HEAD 失敗・ヘッダー欠落時は全文 GET へ進む。subject.txt は取得せず、板先読みも起動しない。
  スレを開くだけなら保存済み dat を表示する。
- **板同期**: `src/sync.rs` が `refresh_board_with_subject` を呼び、板ごとに subject.txt を
  1 回取得する。非 archived・非 dead のお気に入りについて、subject の件数が
  **保存 blob の実レス数**を超える場合に dat を全文取得する。subject にスレがなければ
  dat 取得へ進むが、subject 自体の取得失敗時はその板の更新を中止する。
  本番の一括更新 UI / `POST /api/favorites/refresh` は撤去済みで、総合テストは
  `/_control/refresh-board` からこの経路を駆動する。

両経路とも dat 取得中の重複ダウンロードを抑止し、取得できた本文は `persist_fetch` で
blob 全置換・メタデータ更新を行う。実装は
[reload handler](../src/routes/favorites.rs)、[HEAD / dat 取得](../src/fivech/http.rs)、
[板更新・保存処理](../src/fivech/refresh.rs)、[バックグラウンド同期](../src/sync.rs)を参照。

[tests/backend.rs](../tests/backend.rs) の reload の試験は HEAD サイズ一致時の取得省略、
増加時の取得、いずれも subject 取得が 0 回であることを検証する。refresh の試験は同じ板の
伸びた 3 スレを subject 1 回で更新し、subject 件数が blob と同じスレは取得しないことを検証する。

### 代表シナリオ:「111 止まり」の実フロー再現

[tests/page/stack.rs](../tests/page/stack.rs) の `footer refresh heals a drifted favorite`。

1. `seed-favorite` で メタ `res_count=117` / `blob_posts=111`(ドリフト状態)を投入。
   `dat_bytes` は 111 レス分の Shift_JIS バイト数として保存される。
2. モックに `res_count=117` / `dat_posts=117` を設定。
3. スレを開き、フッターの「更新」を押す → ビューアの reload(GET)が走る。
4. HEAD の `Content-Length` が保存済み `dat_bytes` と異なるため dat 全文取得 → blob 全置換。
5. 更新後の保存済み dat を読み、画面に 117 レス目(本文117)が表示される。

これによりメタの件数が先行していても、HEAD 比較 → dat 取得 → DB 置換 → 表示まで
実バックエンドとローカル 5ch モックを通して回復することを検証する。
