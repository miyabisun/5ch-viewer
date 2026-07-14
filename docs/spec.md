# viewer-of-5ch 仕様書

本アプリ（5ch をスクレイピングして閲覧する Web サービス）の仕様。
**5ch 側の外部仕様（ドメイン・dat 形式・アクセス作法・ff5ch.syoboi.jp 等）は
[5ch-spec.md](5ch-spec.md) に分離**してある。本書はアプリ自身の設計に集中する。

## 参照リポジトリ

| 短縮名 | リポジトリ | 役割 |
| --- | --- | --- |
| sentinel | https://github.com/miyabisun/5ch-sentinel | スレッド監視・次スレ取得の移植元（Node.js） |
| novel-server | https://github.com/miyabisun/novel-server | 技術スタックの参考（Rust + Svelte + SQLite） |

## 1. 背景と目的

スマホの ChMate は dat を端末ローカルに溜めるため、外で読んだスレの続きを帰宅後に
PC やタブレットで読めない。そこで VPS 上に Web サービスを構築し、**どの端末からでも
同じ既読位置で続きを読める**ようにする。dat と既読位置をサーバー（SQLite）に集約し、
端末間で閲覧状態を同期する。

### ゴール
- お気に入りスレをどの端末からでも同じ既読位置で読める
- 5ch への負荷を抑える（無人監視の頻度と転送量を最小化。詳細 6 章）
- スレが埋まったら次スレを自動検出してお気に入りに追加する
- 低スペック VPS で安定動作する

## 2. スコープ

### やること
- お気に入りスレの本文閲覧と既読位置の端末間同期
- dat を Range 差分取得し SQLite に一元管理
- スレタイ検索（ff5ch.syoboi.jp をラップ）と URL 直接登録
- スレッド監視と次スレ自動取得（sentinel から移植）
- お気に入りの星評価（0〜5）による分類表示

### やらないこと（スコープ外）
- マルチユーザー対応（**個人専用・シングルユーザー**）
- 板一覧→スレ一覧の常設ブラウジング UI（将来拡張）
- 5ch への書き込み（将来拡張の候補）
- 外部通知（次スレ検出はお気に入り自動追加で代替）

## 3. 用語

| 用語 | 意味 |
| --- | --- |
| board（板） | スレッドの集合。`server`/`board` の組で識別（例 `egg`/`applism`） |
| board_name | 板の日本語名（SETTING.TXT の BBS_TITLE）。**表示用**でソートには使わない |
| favorites | お気に入り登録したスレッド（本アプリの管理単位） |
| rating | スレの星評価 0〜5。一覧のグループ分けに使う |
| read_res | 既読位置（最後に読んだレス番号） |
| dat_bytes | 前回取得時の 5ch 上の dat サイズ（Shift_JIS バイト数）。次回 Range の開始位置 |

5ch 側の用語（subject.txt / dat / thread_id 等）と形式は [5ch-spec.md](5ch-spec.md)。

## 4. 全体アーキテクチャ

novel-server を踏襲し、**1 つの Rust バイナリ**でフロント配信・API・監視を担う。

```
┌─────────────────────────────────────────────┐
│ VPS                                          │
│  ┌────────────────────────────────────────┐ │
│  │ viewer-of-5ch (single Rust binary)      │ │
│  │  ┌──────────┐   ┌──────────────────┐    │ │
│  │  │ axum     │   │ background sync    │   │ │
│  │  │ - SPA配信 │   │ (tokio interval)   │   │ │
│  │  │ - API    │   │ - スレ監視/次スレ    │   │ │
│  │  └────┬─────┘   └─────────┬──────────┘   │ │
│  │       ▼                   ▼              │ │
│  │  ┌──────────────────────────────────┐   │ │
│  │  │ SQLite (WAL) : favorites/dat_blobs │  │ │
│  │  └──────────────────────────────────┘   │ │
│  │       ▼ reqwest (Range GET / subject)    │ │
│  └───────┼─────────────────────────────────┘ │
└──────────┼────────────────────────────────────┘
           ▼  *.5ch.io
```

認証はアプリでは行わず前段に委譲する（9 章）。クライアント（Svelte SPA）は `/api/*`
を fetch で叩く。dat 本文・既読位置はすべてサーバーの SQLite にあるため、どの端末から
でも同じ状態が得られる。

## 5. 技術スタック

### バックエンド（Rust）
| クレート | 用途 |
| --- | --- |
| axum 0.8 | Web フレームワーク |
| tokio 1 (full) | 非同期ランタイム・定期実行 |
| rusqlite 0.33 (bundled) | SQLite。プールは使わず `Mutex<Connection>` |
| reqwest 0.12 | HTTP クライアント（Range・UA 制御） |
| scraper 0.21 | 投稿フォーム HTML のパース（post.rs） |
| quick-xml 0.36 | ff5ch.syoboi.jp の RSS パース |
| ammonia 4 | レス本文の HTML サニタイズ |
| encoding_rs | Shift_JIS ↔ UTF-8 |
| serde / serde_json | JSON |
| tower-http 0.6 | 静的配信・トレーシング |
| regex-lite / urlencoding / chrono / tracing / tracing-subscriber / thiserror / dotenvy | 補助 |

リリースビルドは `lto=true / strip=true / codegen-units=1`（VPS 向け）。

### フロントエンド
- Svelte 5（SvelteKit 不使用）+ Vite。`client/build/` を Rust が静的配信、未マッチは
  SPA fallback。通信は `/api/*` への fetch（SSR なし）。

### デプロイ
- Docker マルチステージビルド。環境変数 `PORT` / `DATABASE_PATH` / 任意 `BASE_PATH`。

## 6. 5ch アクセス戦略

> 5ch 側の作法（UA・identity・URL 形式・dat 形式等）は [5ch-spec.md](5ch-spec.md)。
> 本章は **アプリがいつ・どう取得するか** の戦略を扱う。

### 6.1 方針
- **ユーザー操作起点（リロード・検索）は遠慮なく取得**してよい。人間の操作ペースが
  自然なレート制限になる（ChMate 等と同じ）。
- 本当に効くのは 2 点だけ:
  1. **無人のバックグラウンド監視の頻度**（誰も見ていなくても動くため、間隔と板単位
     グルーピングで抑える。8.3）
  2. **転送量の最小化**（dat の Range 差分取得。回数でなく 1 回あたりの量を減らす）

### 6.2 アクセスのトリガー
| トリガー | 問い合わせ |
| --- | --- |
| スレを開く / 下に引っ張ってリロード | 該当スレの dat を Range 差分取得 |
| お気に入り登録 | 板の SETTING.TXT を 1 回（board_name 取得） |
| スレタイ検索 | ff5ch.syoboi.jp をラップ（第三者） |
| 更新ボタン（一括更新） | 板ごとに subject.txt を 1 回 + 伸びたスレの dat のみ DL |
| バックグラウンド監視 | 更新ボタンと同じ経路（板 subject.txt 1 回 + 伸びた dat DL） |

一覧ページを開いただけでは 5ch へ一切アクセスしない（SQLite の内容だけで即描画）。
一括チェック（subject + 伸びた dat）が走るのは更新ボタン押下時とバックグラウンド巡回のみ。
ブラウザ標準の引っ張り更新はページ再読み込み＝再描画（GET /api/favorites のみ）になる。

### 6.3 dat の Range 差分取得と整合性（4層防御）

`dat_bytes`（前回の Shift_JIS バイト数）を起点に `Range: bytes=N-` で増分を取得し、
生 Shift_JIS の BLOB 末尾に追記する。あぼーん等で過去が書き換わると差分が壊れるため、
**4 層で整合性を検証し、いずれか NG なら全取得でリペア**する:

1. **末尾6バイト境界一致** — `dat_bytes - 6` から取得し、先頭6バイトが前回 BLOB の末尾
   6バイトと一致するか。dat 末尾は必ず改行終端（5ch-spec 参照）なので1バイトでは衝突
   するため6バイト。
2. **差分ヘッダー検証** — 増分を行分割し各行を `<>` で split。フィールド4未満／日付ID
   欠損を弾く（サイズが偶然微増した壊れ差分を撃墜）。
3. **res_count 退行検出** — 追記後の総レス数が前回を下回れば中間レス物理削除とみなす。
4. **416 縮小検出** — サーバーサイズ ≤ N。Content-Range で同一サイズなら変化なし、
   それ以外は全取得。

実装: `fivech/http.rs::fetch_dat`、`fivech/dat.rs::validate_diff`、`routes/favorites.rs`。

### 6.4 SSRF 対策（入力検証）
`server`/`board`/`thread_id` がユーザー入力（URL 貼付・直接指定・検索結果）から URL
組み立てに入るため、全入力経路で `fivech/url.rs::validate_ref`
（server/board=`^[a-zA-Z0-9_-]+$`、thread_id=`^[0-9]+$`）を通す（多層防御）。

## 7. データモデル（SQLite）

WAL + `cache_size`/`temp_store` 調整（novel-server 踏襲）。

### favorites
```sql
CREATE TABLE IF NOT EXISTS favorites (
    thread_id   TEXT NOT NULL,
    server      TEXT NOT NULL,
    board       TEXT NOT NULL,
    board_name  TEXT NOT NULL,            -- 表示用（ChMate 互換）
    title       TEXT NOT NULL,
    res_count   INTEGER NOT NULL DEFAULT 0,
    dat_bytes   INTEGER NOT NULL DEFAULT 0, -- 次回 Range 開始位置（SJIS バイト数）
    read_res    INTEGER NOT NULL DEFAULT 0,
    rating      INTEGER NOT NULL DEFAULT 0, -- 星 0〜5
    status      TEXT NOT NULL DEFAULT 'active', -- active / warned / dead
    created_at  INTEGER DEFAULT (strftime('%s','now')),
    updated_at  INTEGER DEFAULT (strftime('%s','now')),
    PRIMARY KEY (server, board, thread_id)
);
CREATE INDEX IF NOT EXISTS idx_favorites_order ON favorites (rating DESC, title ASC);
```

### dat_blobs（dat 本体・1:1）
```sql
CREATE TABLE IF NOT EXISTS dat_blobs (
    server TEXT NOT NULL, board TEXT NOT NULL, thread_id TEXT NOT NULL,
    raw    BLOB NOT NULL,                 -- 生 Shift_JIS（追記専用）
    PRIMARY KEY (server, board, thread_id),
    FOREIGN KEY (server, board, thread_id)
        REFERENCES favorites(server, board, thread_id) ON DELETE CASCADE
);
```

> 生 Shift_JIS を保持するのが要点。Range 開始位置（`dat_bytes`）が Shift_JIS バイト数
> なので、保存も生バイトで一致させ、表示時にデコードする。

### status 遷移
- `active` 通常 / `warned` 終了間近（res≥980 または 900KB）/ `dead` 終了（res≥1000
  または 1024KB、または dat 消失=404）。
- 次スレ検出時は旧スレを `dead` にし新スレを追加（rating を継承）。

## 8. 機能要件

### 8.1 お気に入り閲覧・同期と表示
- favorites を **rating でグループ化**して表示。順序は rating 降順 → title 昇順。
- **ソートはフロント責務**。Rust は CRUD に専念し順不同 JSON を返す。自然順
  （`Part2 < Part10`）は `Intl.Collator('ja',{numeric:true})` を使う（自前実装不要）。
- 一覧レイアウト（ChMate 準拠）: 星をグループ見出し、1 行目スレタイ、2 行目
  `board_name + 総レス数`、未読数は `[6]` ラベルを右寄せ（既読時は非表示）。
- スレを開くと dat_blobs を UTF-8 デコードして表示。本文は **サーバーで HTML
  サニタイズ**（ammonia、許可 `a`/`br`）。アンカー `>>123` はクリックでモーダル表示
  （ツリー化の口あり）。
- レスは **最新→最古** の順に表示する。初回は最新レスから前回既読レスまでを優先して
  完成形で描画し、最新レスの上に「おわり」、前回既読レスの直前に「前回ここまで」を置く。
  新着部分が表示領域を満たす場合は「前回ここまで」を下端に合わせる。満たさない場合は空白で
  押し下げず、過去レスを50件ずつ先行表示して自然に画面を満たす。残りはアイドル時に下へ
  追加し、現在のスクロール位置を動かさない。
- 「下に引っ張ってリロード」で 6.3 の Range 差分取得を実行。
- **既読位置の追跡**: 各レスを IntersectionObserver で監視し、画面を通過した最大レス
  番号を `read_res` 候補に。粒度は**レス番号単位**（端末非依存）。送信は debounce
  （2 秒）で間引き、離脱時は `navigator.sendBeacon()` で確実に送る。正本はサーバー。

### 8.2 スレッドの追加
- **(A) スレタイ検索**: ff5ch.syoboi.jp をサーバーでラップ（CORS 回避、精度のため第三者を採用）。`GET /api/search`
  で全 5ch 横断検索 → 候補一覧 → 選んで登録。結果 URL から server/board/thread_id を
  抽出（5ch-spec 参照、`parse_thread_url` 再利用）。
- **(B) URL 直接登録**（ChMate 移行用）: スレ URL を貼り付け、`parse_thread_url` で
  分解。登録時に SETTING.TXT で board_name を取得。

### 8.3 スレッド監視・次スレ自動取得（sentinel 移植）
バックグラウンドで `tokio::time::interval`（180 秒）により、更新ボタンと同じ板更新経路
（`refresh_board_with_subject`）を板ごとに実行:
1. favorites を板単位にグループ化、板ごとに subject.txt を 1 回取得（共有）。
2. subject 件数 > 保存 blob 件数のスレのみ dat を DL し、blob を置換。
3. `persist_fetch` が blob の実レス数から res_count/status/title を更新（唯一の書き手）。
4. warned/dead で subject から次スレを検出し、見つかれば rating 継承で自動追加。

**不変条件**: `favorites.res_count` は保存済み dat（blob）の実レス数のみを反映する。
subject.txt の数字だけで res_count を動かすことはない（`persist_fetch` が唯一の書き手）。
これにより「res_count と blob の乖離」（過去の stuck-at-111 系バグ）が設計上発生しない。
subject 由来の res_count は needs_fetch ゲート・warned/dead 判定・次スレ探索の参照専用。
次スレ・find-next で新規登録するスレの res_count は初期値 0（DEFAULT）で入り、巡回が dat を
落とした時に blob 件数へ更新される。

dead 化したスレも「次スレ探索のみ」を最終更新から 7 日間だけ継続する（dead 化の tick
より後に次スレが立つ取りこぼしレース対策）。この探索は read-only で dead 行の
res_count/status/updated_at を書き換えない（updated_at を触ると 7 日窓が無限延長するため）。
次スレが登録されれば新スレ（active）が板を監視対象に保ち、dead 元スレは 7 日で自然に脱落、
アーカイブ（archived=1）でも対象外になる。

手動救済として `POST …/{…}/find-next` を用意（ユーザー操作起点、subject 1 回取得）。
dead/archived でも呼べる。

巡回間隔 180 秒: dat 本体まで落とすため 1 tick のコストは上がるが、一覧マウント時の
自動更新を廃止した（訪問時アクセス 0）ぶん総アクセスは減る。interval の初回 tick は
即発火するので起動直後に 1 回巡回が走り、初回訪問時点で dat がウォームになる。

### 8.4 次スレ判定（`find-next-thread` 移植）
1. タイトル中の数字を位置付きで全抽出。
2. **右端の数字**から +1 を試す。
3. その数字の前後最大6文字を文脈とし、候補が文脈と +1 後の数値の両方を含むかで判定
   （誤爆防止）。照合は空白非依存（候補・文脈の全空白を除去して比較）で、スレ建て主の
   空白ゆらぎ（例: `★503 【転載禁止】` vs `★504【転載禁止】`）に強い。
4. ゼロパディング（Part09→Part10）も試す。

移植元のテストケースも移植済み。

## 9. 認証
個人専用のため **認証はアプリに実装せず、前段の Cloudflare Access に委譲**する。
アプリは「前段で認証済み」前提で、必要なら `Cf-Access-Authenticated-User-Email` を
読む程度。認証基盤の改修方針は home-server リポジトリの
`docs/cloudflare-access-setup.md`。

## 10. API 設計
| メソッド | パス | 用途 |
| --- | --- | --- |
| GET | `/api/favorites` | お気に入り一覧（順不同。並べ替えはフロント） |
| POST | `/api/favorites` | 追加（URL 直接 or server/board/thread_id） |
| DELETE | `/api/favorites/{server}/{board}/{thread_id}` | 削除 |
| GET | `…/{…}/dat` | 保存済み dat（サニタイズ済みレス配列） |
| POST | `…/{…}/reload` | Range 差分取得を実行 |
| PATCH | `…/{…}/progress` | 既読位置 `read_res` 更新 |
| PATCH | `…/{…}/rating` | 星評価更新 |
| POST | `…/{…}/find-next` | 次スレを手動検索（subject 1 回取得、見つかれば rating 継承で登録） |
| GET | `/api/search?q={keyword}` | スレタイ検索（ff5ch.syoboi.jp ラップ） |

## 11. 非機能要件
- **メモリ**: dat 全体を常駐させず必要時に SQLite から読む。`cache_size` は実測調整。
- **同時実行**: 個人利用のため `Mutex<Connection>` で十分。WAL で読み書き並行性確保。
- **5ch への礼儀**: 無人監視の間隔・リトライ間隔を保守的に。

## 12. sentinel からの移植マップ
| 移植元（Node.js） | 移植先（Rust） |
| --- | --- |
| `functions/parse-subject.js` | `fivech/subject.rs` |
| `functions/find-next-thread.js` | `fivech/next_thread.rs`（テストも移植） |
| `functions/parse-thread-url.js` | `fivech/url.rs` |
| `functions/group-threads-by-board.js` | `sync.rs`（板グルーピング） |
| `modules/http.js` | `fivech/http.rs`（UA/identity/リトライ/404 非リトライ） |
| `modules/checker.js` | `sync.rs`（状態遷移・次スレ検出・自動追加） |
| `modules/discord.js` | 移植しない（通知は自動追加で代替） |

## 13. 将来拡張（スコープ外・候補）
- 板一覧→スレ一覧の常設ブラウジング
- 5ch への書き込み
- アンカーのツリー表示（現状は1段モーダル）
- ff5ch.syoboi.jp 結果の短 TTL キャッシュ
- NG ワード / 画像プレビュー
