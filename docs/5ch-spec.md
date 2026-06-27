# 5ch 仕様（調査メモ）

本アプリが 5ch をスクレイピングするために調査・収集した **5ch 側の外部仕様**。
これは「本アプリの仕様」ではなく「相手（5ch）の仕様」であり、5ch 側の変更で陳腐化
しうる。本アプリ自身の仕様は [spec.md](spec.md)。

## ドメイン（5ch.io）

- **2026-03-06、米レジストラ Epik が 5ch.net を停止**（生き物苦手板の動物虐待
  コンテンツ放置が原因）。運営は **5ch.io へ移転**した。
- 5ch.net は現在 5ch.io へ 301 リダイレクトするが、剥奪済みドメインのため恒久的な
  保証はない。**スクレイピングは 5ch.io を正とする**。
- サブドメイン（`kizuna`, `eagle` 等＝サーバー名）と板の対応は移転前後で**不変**。
  ホスト名のサフィックスが `.5ch.net` → `.5ch.io` に変わっただけ。
- 板一覧: `https://menu.5ch.io/bbsmenu.html`
- 過去ログ URL には `.5ch.net` 形式が残るため、URL パーサは net/io 両方を受理する。

## HTTP アクセスの作法

- **User-Agent に `Monazilla/1.00` を含める**。含めないとブロック（403）。
  例: `Monazilla/1.00 Mozilla/5.0 (Windows NT 10.0; Win64; x64) ...`
- **`Accept-Encoding: identity` を付ける**。付けないと前段の Cloudflare が gzip
  圧縮レスポンスを返し、`Content-Length` が消えてサイズ判定・Range 取得が壊れる。
- レスポンスは **Shift_JIS**。
- 5xx はリトライ可。**404 はリトライしない**（dat 消失＝スレ落ち）。
- ff5ch.syoboi.jp を含む外部サービス（subject/dat/SETTING）はブラウザ判定でなく Monazilla UA で通る。

## subject.txt（板のスレッド一覧）

- `https://{server}.5ch.io/{board}/subject.txt`
- 1 行 = 1 スレッド: `{thread_id}.dat<>{title} ({res_count})`
- パース正規表現: `^(\d+)\.dat<>(.+)\((\d+)\)$`
  - greedy な `.+` が最後の `(` まで取り込むため、タイトル中の括弧はタイトルの一部に
    なり、末尾の `(数字)` だけが res_count になる。

## dat（スレッド本文）

- `https://{server}.5ch.io/{board}/dat/{thread_id}.dat`
- `thread_id` = スレッド作成時刻の UNIX 秒（dat ファイル名にもなる）。
- 1 行 = 1 レス、`<>` 区切り:
  `名前<>メール<>日付 ID等<>本文<>スレッドタイトル(1レス目のみ)`
- レス番号 = 行番号（1 始まり）。
- **末尾は必ず改行(0x0a)終端**（実機確認済み）。追記専用。Shift_JIS。
- 本文中の `>>123` はアンカー（エンティティ化され `&gt;&gt;123` で来ることが多い）。
  本文には `<br>` や `<a>` 等の HTML タグが含まれうる。

## dat の Range 差分取得

- `Range: bytes=N-`（N = 前回取得時の Shift_JIS バイト数）で増分のみ取得できる。
- ステータス:
  - **206** = N バイト目以降の増分
  - **200** = 全体（サーバーが Range を無視）
  - **416** = サーバー側サイズ ≤ N。`Content-Range: bytes */{total}` で実サイズが
    分かる場合がある（**返さないサーバーもあり要実機確認**）。
  - **404** = スレ落ち
- **あぼーん**: レス削除で dat が縮む、または該当行が別文字列に置換される。バイト数が
  ずれて後続が全体的に動くことが多いため、末尾バイトの境界照合や差分のパースで検知
  できる（本アプリ側の整合性対策＝4層防御は [spec.md](spec.md) 6.3）。

## ワッチョイ（ﾜｯﾁｮｲ）

スレ立て時のコマンドで有効化される強制コテハン。各レスの**名前欄**に
`(ﾜｯﾁｮｲ xxyy-zzzz)` の形で付く（本アプリは `\w{4}-\w{4}` トークンを抽出して扱う）。

### 文字列 `xxyy-zzzz` の構造

| 部分 | 由来 | 変化 |
|---|---|---|
| `xx`（前半の前2桁） | **IP の第1オクテット**（頭3桁）由来の16進 | 毎週木曜にリセット |
| `yy`（前半の後2桁） | **プロバイダのドメイン名**由来の16進 | 同一プロバイダなら不変 |
| `zzzz`（ハイフン後4桁） | **UA（ユーザーエージェント）**由来 | 毎週木曜にリセット。ただし**同一ブラウザなら回線（IP）を変えても不変** |

- **リセット周期**: 毎週**木曜**に salt が変わり、`xx` と `zzzz` が更新される
  （＝有効な「同一人物」シグナルは **木曜〜翌水曜の1週間**に閉じる）。正確なリセット
  時刻は未確定のため、実装では木曜の境界で週を区切る（要・実機確認）。

### 同一人物（自演）判定での使い方

- **前半 `xxyy` は当てにならない**: スマホは家庭 WiFi とモバイル回線を使い分けられ、
  VPS/プロキシ荒らしも IP を自在に変える。IP 由来の `xx` は容易にすり抜けられる。
- **末尾 `-zzzz` が有効**: UA 由来で、同一ブラウザなら回線を変えても変わらない。
  「回線を切り替えて連投する同一端末」を捕捉できる。ChMate 等でも末尾4桁を正規表現
  `/zzzz$/` で NG するのが定石。
- **限界（誤爆要因）**:
  - 毎週木曜にリセットされるため、**週をまたぐと別人**になり得る。
  - 4桁ゆえ**衝突**（別人が同じ `zzzz`）があり得る。
  - 別ブラウザ/別端末/UA 偽装で `zzzz` も変わる（すり抜け可能）。
- **誤爆を抑える運用**: `-zzzz` 単独でなく **板 + 週（木曜〜翌水曜）でスコープ**を切る。
  同じ板・同じ週の中でのみ `-zzzz` 一致を「同一人物」とみなすことで、週次リセットと
  板差による偽陽性を最小化できる。本アプリの NG ワッチョイはこの方針を採る。

### 出典

- [ワッチョイの構造解説（do-not-open.hatenablog.com）](https://do-not-open.hatenablog.com/entry/2017/05/10/002630)
- [ワッチョイとは（ニコニコ大百科）](https://dic.nicovideo.jp/a/%E3%83%AF%E3%83%83%E3%83%81%E3%83%A7%E3%82%A4)

## SETTING.TXT（板情報）

- `https://{server}.5ch.io/{board}/SETTING.TXT`
- `BBS_TITLE=スマホアプリ` の行に板の日本語名。

## スレタイ検索（ff5ch.syoboi.jp）

公式の 5ch 検索エンドポイントは検索精度が低く（無関係スレが上位に来る）、代わりに第三者の
**ff5ch.syoboi.jp**（しょぼいカレンダー運営）を採用している。精度が大幅に優れる
（完全部分一致・新しい順）ため。

- エンドポイント: `https://ff5ch.syoboi.jp/?q={URLエンコード}&alt=rss`
- **User-Agent はプロジェクト標準の Monazilla UA で通る**（ブラウザ UA 不要）。
- レスポンスは **RSS 2.0 XML**（HTML スクレイピング不要）。
- RSS フィールド:

  | 要素 | 内容 |
  |---|---|
  | `<title>` | スレタイ。末尾の `  (N)` がレス数（正規表現 `\((\d+)\)\s*$`） |
  | `<guid>` | 絶対 URL `http://{server}.5ch.io/test/read.cgi/{board}/{thread_id}/` |
  | `<pubdate>` | スレ作成日時 |
  | `<category>` | 板名 |

- `<channel>` 直下にも `<title>` があるため、`<item>` 内の `<title>` のみを採用する。
- `<guid>` は `http://` 始まりだが `parse_thread_url` は `https?://` 両対応のため変換不要。
- **レス数はスレタイ末尾の `(123)`** から取る（タイトル中の `(仮)` 等と区別するため
  「末尾の括弧」のみ。正規表現 `\((\d+)\)\s*$`）。

## スレッドの終了

- 1 スレッド最大 **1000 レス**で書き込み終了。dat サイズ上限の目安は **約 1024KB**。
- 本アプリの閾値（sentinel `resWarningThreshold`/`resDeadThreshold` 準拠）:
  - **警告（warned）** = res ≥ 980（sentinel resWarningThreshold=980 と一致）
  - **終了（dead）** = res ≥ 1002（sentinel resDeadThreshold=1002 準拠。1000/1001 は warned 扱い）
  - 次スレ探索・登録は **warned 時点（res≥980）から**行う（sentinel 同様の早期追尾）。
- **size 次元について（将来メモ）**: sentinel は危険域判定に dat サイズ次元
  （warned = 900KB、dead = 1024KB）と dat 落ち（404 HEAD）も使うが、
  本アプリは **5ch アクセス削減を優先**し当面 res_count のみで判定する。
  荒らし対策（連レスによる 1000 到達加速・dat 1024KB 化）が必要になれば
  size 次元（HEAD で Content-Length 取得）を追加する。
- ※ したらば等は 1000 超（〜10000）を扱う掲示板もあるが、本アプリは **5ch 専用**。

## 次スレの慣習

- タイトル末尾に Part 番号などの連番（例 `Part5843` → `Part5844`）。ゼロパディング
  形式（`Part09` → `Part10`）もある。
- 同一シリーズの判定は「右端の数字を +1 し、その前後の文脈（最大6文字）で一致を確認」
  するのが誤爆を避けやすい（アルゴリズムは [spec.md](spec.md) 8.4）。

## 書き込み（bbs.cgi）

**実機調査済み**（2026-06、`egg/software/1779846933`「[test]書き込みテスト Part40」＝
運営公認の書き込みテストスレで検証）。5ch への投稿は `bbs.cgi` への POST で行う。

### エンドポイントとパラメータ

- **POST 先**: `https://{server}.5ch.io/test/bbs.cgi`
  - 確認ページ経由の再送時は `?guid=ON` を付ける（確認フォームの action がそれ）。
- **Content-Type**: `application/x-www-form-urlencoded`、**本文・値は Shift_JIS**。
  - 日本語（`MESSAGE` や `submit` ボタン文言）は **Shift_JIS にエンコードしてから**
    URL エンコードする。UTF-8 のまま送ると文字化けする。
- **User-Agent**: 取得系と同じ `Monazilla/1.00 ...`（[HTTP アクセスの作法](#http-アクセスの作法)）。
- **Referer**: 当該スレの read.cgi URL を付ける
  （`https://{server}.5ch.io/test/read.cgi/{board}/{key}/`）。
- パラメータ:

  | name | 値 | 備考 |
  |---|---|---|
  | `bbs` | board ID（例 `software`） | 必須 |
  | `key` | thread_id（dat ファイル名の数字） | 必須 |
  | `time` | UNIX 秒 | 任意の最近の値で通る |
  | `FROM` | 名前 | 空可 |
  | `mail` | メール欄（`sage` 等） | 空可 |
  | `MESSAGE` | 本文（Shift_JIS） | 必須 |
  | `submit` | `書き込む`（Shift_JIS） | 必須 |

### 投稿フロー（2 段 + Cookie で 1 段に短縮）

5ch は荒らし対策で **Cookie 未設定だと必ず「書き込み確認」ページ**を返す。

1. **1 回目 POST** → 確認ページが返る。判定材料:
   - レスポンスヘッダ **`x-chx-error: 0000 Confirmation`**
   - `<title>■ 書き込み確認 ■</title>`、本文に「書きこみ＆クッキー確認」
   - 確認フォームに hidden の **`feature` = `confirmed:<40桁hex>`** が含まれる。
2. **2 回目 POST**（`bbs.cgi?guid=ON`）で、1 回目と**同じ**
   `bbs/key/time/FROM/mail/MESSAGE` に加えて
   - `feature=confirmed:<hash>`（確認ページから抽出した値をそのまま）
   - `submit=上記全てを承諾して書き込む`（Shift_JIS）

   を付けて送る → 成功。**成功レスポンスで `Set-Cookie: acorn=...` と
   `MonaTicket=...`** が返る（`.5ch.io` ドメイン、数日有効）。
3. **以降は Cookie（acorn / MonaTicket）を保持して送れば確認ページはスキップ**され、
   通常の `submit=書き込む` だけで **1 回の POST で投稿が通る**（実機確認済み）。

→ 実装方針: Cookie ストアを永続化し、まず 1 発で投げる。**確認ページが返ったら
`feature` を抽出して即再送**し、成功時の Cookie を保存する。次回からは 1 発で通る。

### 成功の判定と「自分のレス番号」

成功レスポンス（`<title>書きこみました。</title>`）には次のヘッダが付く:

| ヘッダ | 例 | 意味 |
|---|---|---|
| `x-resnum` | `406` | **投稿が入ったレス番号**（＝行番号） |
| `x-posterid` | `E7CsJrO3` | そのスレでの自分の ID |
| `x-postdate` | `1781778054.18` | 投稿時刻 |
| `x-postplace` | `software/1779846933` | board/key |

- **`x-resnum` が「自分のレス」同定の決め手**。投稿成功直後に dat にも即時反映される
  （実機: POST 直後の dat 末尾＝`x-resnum` 行）ので、**`(server, board, thread_id,
  res_num)` を保存すれば本文一致や時刻推定は不要**。
- エラー時は `x-chx-error` が `Confirmation` 以外（例: 規制・連投・スレ落ち）になる、
  または確認以外のエラーページ HTML が返る。`x-resnum` の有無＋`title` で成否判定する。

### 注意

- `egg/software/1779846933`「[test]書き込みテスト Part40」は**運営公認の書き込み
  テスト用スレ（砂場）**。動作検証はここへ投稿してよい。
- 取得系（subject/dat/SETTING）は GET 専用（[dat](#dat-スレッド本文)）。投稿系は本節の
  POST 経路を別に設ける。SSRF 対策として server/board/thread_id は既存の
  `validate_ref`（[url.rs] の正規表現）で投稿前に検証すること。
