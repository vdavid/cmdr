/**
 * The teardowns an async `onMount` collects as it goes. Unmounting can land while
 * startup is parked on an await, so a listener registered after that point would
 * otherwise outlive the component: once the scope is disposed, `add` runs the
 * teardown at once and answers `false`, telling the caller to stop.
 */
export interface MountScope {
  /** Registers a teardown. Returns whether the scope is still live; if not, the teardown already ran. */
  add: (teardown: () => void) => boolean
  readonly disposed: boolean
  /** Runs every registered teardown once, newest first. Later `add`s run immediately. */
  dispose: () => void
}

export function createMountScope(): MountScope {
  const teardowns: (() => void)[] = []
  let disposed = false
  return {
    add: (teardown) => {
      if (disposed) {
        teardown()
        return false
      }
      teardowns.push(teardown)
      return true
    },
    get disposed() {
      return disposed
    },
    dispose: () => {
      if (disposed) return
      disposed = true
      for (const teardown of teardowns.reverse()) teardown()
      teardowns.length = 0
    },
  }
}
