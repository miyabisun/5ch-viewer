import { describe, expect, it } from 'vitest'
import { preserveReadProgress } from './favorite-list.js'

describe('preserveReadProgress', () => {
  it('keeps newer local read progress when a refreshed row is stale', () => {
    const current = [
      {
        server: 'egg',
        board: 'software',
        thread_id: '1',
        read_res: 12,
        title: 'old',
      },
    ]
    const refreshed = [
      {
        server: 'egg',
        board: 'software',
        thread_id: '1',
        read_res: 9,
        title: 'new',
      },
    ]

    expect(preserveReadProgress(current, refreshed)).toEqual([
      {
        server: 'egg',
        board: 'software',
        thread_id: '1',
        read_res: 12,
        title: 'new',
      },
    ])
  })

  it('accepts newer server progress and preserves new rows', () => {
    const current = [{ server: 'egg', board: 'software', thread_id: '1', read_res: 4 }]
    const refreshed = [
      { server: 'egg', board: 'software', thread_id: '1', read_res: 7 },
      { server: 'news', board: 'newsplus', thread_id: '2', read_res: 0 },
    ]

    expect(preserveReadProgress(current, refreshed)).toEqual(refreshed)
  })
})
