import { expect, it } from 'vitest'
import { readImageEntry, withImageEntry } from './image-history.js'

const path = '/egg/applism/123'
const image = { resNum: 35, indexInRes: 1 }

it('stores a serializable image location while retaining unrelated history state', () => {
  const original = { other: { value: 7 } }
  const state = withImageEntry(original, path, { ...image, images: ['not stored'] })
  expect(state).toEqual({ ...original, imageViewer: { path, ...image } })
  expect(original).toEqual({ other: { value: 7 } })
  expect(readImageEntry(structuredClone(state), path)).toEqual({ path, ...image })
  expect(withImageEntry(null, path, image)).toEqual({ imageViewer: { path, ...image } })
})

it('does not treat another thread, missing or malformed data as an image entry', () => {
  expect(readImageEntry(withImageEntry(null, path, image), '/egg/applism/456')).toBeNull()
  for (const state of [
    null,
    {},
    { imageViewer: null },
    ...[
      { path, resNum: 0, indexInRes: 0 },
      { path, resNum: '35', indexInRes: 0 },
      { path, resNum: 35, indexInRes: -1 },
      { path, resNum: 35, indexInRes: 0.5 },
      { path, resNum: 35 },
    ].map((imageViewer) => ({ imageViewer })),
  ]) {
    expect(readImageEntry(state, path)).toBeNull()
  }
})
