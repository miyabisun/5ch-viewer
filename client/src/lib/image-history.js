export function readImageEntry(state, path) {
  const entry = state?.imageViewer
  return entry?.path === path &&
    Number.isSafeInteger(entry.resNum) &&
    entry.resNum > 0 &&
    Number.isSafeInteger(entry.indexInRes) &&
    entry.indexInRes >= 0
    ? entry
    : null
}

export function withImageEntry(state, path, { resNum, indexInRes }) {
  return { ...state, imageViewer: { path, resNum, indexInRes } }
}
