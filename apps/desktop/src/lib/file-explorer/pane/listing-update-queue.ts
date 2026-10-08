/** Filter switches and watcher reconciliation must own the pane's row indices across every await. */
export function createListingUpdateQueue() {
  let tail: Promise<void> | undefined
  return function runListingUpdate<T>(update: () => Promise<T>): Promise<T> {
    const result = tail ? tail.then(update) : update()
    // A refused IPC must not strand the updates queued behind it.
    const settled = result.then(
      () => {},
      () => {},
    )
    tail = settled
    void settled.then(() => {
      if (tail === settled) tail = undefined
    })
    return result
  }
}

export type ListingUpdateQueue = ReturnType<typeof createListingUpdateQueue>
