# テスト構成

このプロジェクトのテストは「層ごとに速さと網羅を分ける」方針。

| 層 | コマンド | 何を検証するか | バックエンド |
| --- | --- | --- | --- |
| Rust 単体 | `cargo test` | パーサ / DB / reload ゲートのロジック | 実 DB(:memory:)・HTTP なし |
| フロント単体 | `cd client && bun run test` (vitest) | 純関数(name 整形 等) | なし |
| フロント E2E(高速) | `cd client && bun run test:e2e` | UI 挙動。API は `page.route` で全モック | なし(モック) |
| 総合テスト(full-stack) | `cd client && bun run test:integration` | Svelte → 実 Rust → 実 DB → 5ch モックの一気通貫 | 実 Rust + :memory: DB + 5ch モック |

「テスト緑 = 実機で直る」を担保するのが総合テスト。reload ゲート / dat 取得 / DB 全置換 /
お気に入り表示など、バックエンドが絡む回帰は総合テスト側で実フロー検証する。

## 5ch アクセスの差し替え

`src/goch/http.rs` は通常 `https://{server}.5ch.io` にアクセスするが、
**環境変数 `GOCH_BASE_URL`** を設定すると全リクエストをその origin に向ける。

- 本番: 未設定 → `https://{server}.5ch.io`(従来どおり)
- テスト: `GOCH_BASE_URL=http://127.0.0.1:3002` → ローカルのモック 5ch へ

差し替えても SSRF 検証(`validate_ref`)・Monazilla UA・Shift_JIS デコード・
`Accept-Encoding: identity` はそのまま。変わるのはホスト部分のみ
(`/{board}/subject.txt`・`/{board}/dat/{thread_id}.dat`・`/{board}/SETTING.TXT` のパスは固定)。

## テストサーバー(itest-server)

`src/bin/itest-server.rs` が 1 プロセスで 2 つの HTTP サーバーを立てる。

- アプリ本体: `routes::build_router` をそのまま使い、**インメモリ SQLite**(`:memory:`、
  単一 Connection なのでプロセス生存中は保持)で起動。`goch_base_url` をモックへ向ける。
  バックグラウンド同期(`start_sync`)は起動しない(60 秒ポーリングは総合テストの
  決定性を損なうため。reload はテストが明示的に駆動する)。
- モック 5ch: `subject.txt` / dat / `SETTING.TXT` を返す。レス数や dat 消失(404)を
  実行時に差し替え可能。

ポートは環境変数で変更可:

```
APP_PORT=3001 MOCK_PORT=3002 cargo run --bin itest-server
```

本番(3000)とは別ポート・別 DB(:memory:)なので、本番データには一切触れない。

### 制御エンドポイント(テスト専用)

アプリ側(`APP_PORT`):

- `POST /_control/reset` — favorites を全削除(dat_blobs は FK CASCADE で連動削除)。各テスト前に呼ぶ。
- `POST /_control/seed-favorite`
  `{ server, board, thread_id, title, res_count, blob_posts }`
  favorite と dat_blobs を投入。`res_count`(メタ)と `blob_posts`(実 blob のレス数)を
  別々に指定でき、**ドリフト**(メタ 117 / blob 111 など)を再現できる。

モック側(`MOCK_PORT`):

- `POST /_control/thread`
  `{ server, board, thread_id, title, res_count, dat_posts, gone }`
  subject.txt が報告するレス数(`res_count`)と dat が返すレス数(`dat_posts`)を指定。
  `gone: true` で dat を 404 にしてスレ落ちを再現。
- `POST /_control/reset` — モックのスレ定義と subject ヒットカウンタを全消去。
- `GET /_control/subject-hits/{board}` — その板の subject.txt が叩かれた回数を返す。
  「板単位の先読み」が subject.txt を**板 1 回**に抑える(スレ数分叩かない)ことを
  検証するためのカウンタ。

## 総合テストの動かし方

Playwright が 3 プロセス(Rust テストサーバー / 5ch モック / Vite dev)を自動起動する。

```
cd client && bun run test:integration
```

構成(`client/playwright.integration.config.js`):

- Vite dev(:5174)が `/api` を `http://127.0.0.1:3001`(itest-server)へプロキシ。
  プロキシ先は環境変数 `VITE_API_TARGET` で切り替え(`vite.config.js`)。
- フロントは普段どおり `/api/...` を叩くだけ。実 Rust に届く。
- 既存の高速 E2E(`playwright.config.js`、`testDir: ./tests`)とは設定・ディレクトリが
  分離(総合は `testDir: ./integration`)。互いに影響しない。

### 代表シナリオ:「111 止まり」の実フロー再現

`client/integration/reload.spec.js`:

1. `seed-favorite` で メタ `res_count=117` / `blob_posts=111`(ドリフト状態)を投入。
2. モックに subject=117 / dat=117 を設定。
3. スレを開く → ビューアの reload(GET)が走る。
4. ゲートは **blob のレス数(111)** を基準に判定し 117 > 111 で dat 取得 → blob 全置換。
5. 画面に 117 レス目(本文117)が表示される。

これにより「メタが先行ドリフトすると blob が更新されず 111 で止まる」バグを、
モックでなく実バックエンドのフローで再現・防止する。
