<script>
  import { untrack } from 'svelte'
  import { api, beaconProgress } from './api.js'

  let { fav, onback } = $props()

  let data = $state(null)
  let loading = $state(false)
  // 既読位置（画面を通過した最大レス番号）。初期値は保存済み read_res（初回のみ）。
  let maxRead = $state(untrack(() => fav.read_res))

  async function load() {
    data = await api.getDat(fav.server, fav.board, fav.thread_id)
    if (data.read_res > maxRead) maxRead = data.read_res
  }

  async function reload() {
    loading = true
    try {
      await api.reload(fav.server, fav.board, fav.thread_id)
      await load()
    } catch (e) {
      alert(e.message)
    } finally {
      loading = false
    }
  }

  // 初回読み込み。
  $effect(() => {
    load()
  })

  // IntersectionObserver で可視レスを追跡し maxRead を更新。
  // NOTE: 雛形実装。action 実行時に observer が未生成のケースは
  //       一覧再描画では起きないが、厳密化は要検討（docs 参照）。
  let observer
  $effect(() => {
    observer = new IntersectionObserver((entries) => {
      for (const e of entries) {
        if (e.isIntersecting) {
          const n = Number(e.target.dataset.res)
          if (n > maxRead) maxRead = n
        }
      }
    })
    return () => observer?.disconnect()
  })

  function track(node, num) {
    node.dataset.res = num
    let obs = observer
    obs?.observe(node)
    return {
      destroy() {
        obs?.unobserve(node)
      },
    }
  }

  // debounce 送信（スクロール停止 2s 後）。
  let timer
  $effect(() => {
    const n = maxRead
    clearTimeout(timer)
    timer = setTimeout(() => {
      api.setProgress(fav.server, fav.board, fav.thread_id, n).catch(() => {})
    }, 2000)
    return () => clearTimeout(timer)
  })

  // 離脱時は sendBeacon で確実に最終位置を送る。
  $effect(() => {
    const sendBeacon = () =>
      beaconProgress(fav.server, fav.board, fav.thread_id, maxRead)
    const onHide = () => {
      if (document.visibilityState === 'hidden') sendBeacon()
    }
    const onPageHide = sendBeacon
    document.addEventListener('visibilitychange', onHide)
    window.addEventListener('pagehide', onPageHide)
    return () => {
      document.removeEventListener('visibilitychange', onHide)
      window.removeEventListener('pagehide', onPageHide)
    }
  })

  // 本文中のアンカー >>N。本文はサニタイズ済みなので >> は &gt;&gt; になっている。
  const ANCHOR_RE = /(?:&gt;){2}(\d+)/g

  // アンカー(>>123)をクリック可能な span に変換（本文はサーバーでサニタイズ済み）。
  // data-anchor は数字のみなので新たな XSS 経路は生じない。
  function linkify(html) {
    return html.replace(
      ANCHOR_RE,
      '<span class="anchor" data-anchor="$1">&gt;&gt;$1</span>',
    )
  }

  // 逆参照マップ: N -> [N にアンカーしているレス番号...]。
  // 各レス本文の >>N を解析してフロントで集計する（サーバー変更不要）。
  const backrefs = $derived.by(() => {
    const map = new Map()
    if (!data?.res) return map
    for (const r of data.res) {
      const seen = new Set()
      let m
      while ((m = ANCHOR_RE.exec(r.body)) !== null) {
        const target = Number(m[1])
        if (target === r.num || seen.has(target)) continue
        seen.add(target)
        if (!map.has(target)) map.set(target, [])
        map.get(target).push(r.num)
      }
    }
    return map
  })

  // 番号からレスを引く（無ければ missing）。
  function resOf(num) {
    return data?.res.find((r) => r.num === num) ?? { num, missing: true }
  }

  // モーダルはスタック式。先頭が現在表示中のレス。
  // アンカー先/元のどちらをタップしても push し、「戻る」で pop する。
  let anchorStack = $state([])
  const currentAnchor = $derived(anchorStack[anchorStack.length - 1] ?? null)

  function openAnchor(num) {
    anchorStack = [...anchorStack, resOf(num)]
  }
  function popAnchor() {
    anchorStack = anchorStack.slice(0, -1)
  }
  function closeAnchor() {
    anchorStack = []
  }

  // 本文クリック（一覧・モーダル共通）。アンカーをタップしたら辿る。
  function onBodyClick(e) {
    const a = e.target.closest('.anchor')
    if (!a) return
    openAnchor(Number(a.dataset.anchor))
  }
</script>

<div class="bar">
  <button onclick={onback}>← 戻る</button>
  <button onclick={reload} disabled={loading}>{loading ? '更新中…' : '↻ 更新'}</button>
</div>

<!-- body はサーバーでサニタイズ済み。linkify でアンカーをクリック可能化。 -->
{#snippet body(html)}
  <div class="body" role="presentation" onclick={onBodyClick}>{@html linkify(html)}</div>
{/snippet}

<!-- 逆参照（このレスにアンカーしているレス）。タップで辿れる。 -->
{#snippet refs(num)}
  {@const list = backrefs.get(num)}
  {#if list}
    <div class="backrefs" role="presentation" onclick={onBodyClick}>
      {#each list as src}
        <span class="anchor" data-anchor={src}>&gt;&gt;{src}</span>
      {/each}
    </div>
  {/if}
{/snippet}

<!-- レス本体（一覧・モーダル共通）。本文中・逆参照のアンカーを辿れる。 -->
{#snippet resBody(r)}
  <span class="num">{r.num}</span>
  <span class="name">{r.name}</span>
  <span class="date">{r.date}</span>
  {@render body(r.body)}
  {@render refs(r.num)}
{/snippet}

{#if data}
  <h1>{data.title || fav.title}</h1>
  {#each data.res as r (r.num)}
    <div class="res" use:track={r.num} class:unread={r.num > fav.read_res}>
      {@render resBody(r)}
    </div>
  {/each}
{/if}

{#if currentAnchor}
  <div class="modal-bg" role="presentation" onclick={closeAnchor}>
    <div class="modal" role="presentation" onclick={(e) => e.stopPropagation()}>
      <div class="modal-bar">
        {#if anchorStack.length > 1}
          <button onclick={popAnchor}>← 戻る</button>
        {/if}
        <button class="close" onclick={closeAnchor}>閉じる</button>
      </div>
      {#if currentAnchor.missing}
        <p>レス {currentAnchor.num} は未取得です</p>
      {:else}
        <!-- モーダル内のアンカーも辿れる（再帰的にスタックへ push）。 -->
        <div class="res">
          {@render resBody(currentAnchor)}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .bar {
    position: sticky;
    top: 0;
    background: #fafafa;
    display: flex;
    gap: 0.5rem;
    padding: 0.5rem 0;
  }
  h1 {
    font-size: 1.1rem;
  }
  .res {
    background: #fff;
    border: 1px solid #eee;
    border-radius: 6px;
    padding: 0.5rem;
    margin-bottom: 0.3rem;
  }
  .res.unread {
    border-left: 3px solid #c00;
  }
  .num {
    font-weight: bold;
    color: #060;
  }
  .name {
    color: #060;
    margin-left: 0.3rem;
  }
  .date {
    font-size: 0.75rem;
    color: #999;
    margin-left: 0.3rem;
  }
  .body {
    margin-top: 0.3rem;
    white-space: pre-wrap;
    word-break: break-word;
  }
  :global(.anchor) {
    color: #1a6;
    cursor: pointer;
    text-decoration: underline;
  }
  .backrefs {
    margin-top: 0.3rem;
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    font-size: 0.8rem;
  }
  .modal-bar {
    display: flex;
    justify-content: space-between;
    gap: 0.5rem;
    margin-bottom: 0.5rem;
  }
  .modal-bar .close {
    margin-left: auto;
  }
  .modal-bg {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
  }
  .modal {
    background: #fff;
    border-radius: 8px;
    padding: 1rem;
    max-width: 100%;
    max-height: 80%;
    overflow: auto;
  }
</style>
