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

  // アンカー(>>123)をクリック可能な span に変換（本文はサーバーでサニタイズ済み）。
  function linkify(html) {
    return html.replace(
      /(?:&gt;){2}(\d+)/g,
      '<span class="anchor" data-anchor="$1">&gt;&gt;$1</span>',
    )
  }

  // アンカーのモーダル表示（簡易版: 該当レス1件）。
  // TODO: アンカー元をツリーで辿る(あにまん/ChMate風)はここを拡張する口。
  let anchorRes = $state(null)
  function onBodyClick(e) {
    const a = e.target.closest('.anchor')
    if (!a) return
    const num = Number(a.dataset.anchor)
    anchorRes = data?.res.find((r) => r.num === num) ?? { num, missing: true }
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

{#if data}
  <h1>{data.title || fav.title}</h1>
  {#each data.res as r (r.num)}
    <div class="res" use:track={r.num} class:unread={r.num > fav.read_res}>
      <span class="num">{r.num}</span>
      <span class="name">{r.name}</span>
      <span class="date">{r.date}</span>
      {@render body(r.body)}
    </div>
  {/each}
{/if}

{#if anchorRes}
  <div class="modal-bg" role="presentation" onclick={() => (anchorRes = null)}>
    <div class="modal" role="presentation" onclick={(e) => e.stopPropagation()}>
      {#if anchorRes.missing}
        <p>レス {anchorRes.num} は未取得です</p>
      {:else}
        <div class="res">
          <span class="num">{anchorRes.num}</span>
          <span class="name">{anchorRes.name}</span>
          <!-- モーダル内のアンカーも辿れる（ツリー化の口） -->
          {@render body(anchorRes.body)}
        </div>
      {/if}
      <button onclick={() => (anchorRes = null)}>閉じる</button>
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
