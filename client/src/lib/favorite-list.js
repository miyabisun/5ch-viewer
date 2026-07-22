function favoriteKey(favorite) {
  return `${favorite.server}/${favorite.board}/${favorite.thread_id}`
}

/** Keep locally observed read progress when a cached list response is stale. */
export function preserveReadProgress(current, refreshed) {
  const currentByKey = new Map(current.map((favorite) => [favoriteKey(favorite), favorite]))

  return refreshed.map((favorite) => {
    const previous = currentByKey.get(favoriteKey(favorite))
    if (!previous || (previous.read_res ?? 0) <= (favorite.read_res ?? 0)) return favorite
    return { ...favorite, read_res: previous.read_res }
  })
}
