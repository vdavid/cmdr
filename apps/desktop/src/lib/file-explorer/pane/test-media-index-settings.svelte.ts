/**
 * Reactive stand-in for the two media-index reads of
 * `$lib/settings/reactive-settings.svelte`, so a test can flip a toggle and have
 * the reader's `$derived` gates re-run exactly as they do at runtime. Lives in a
 * module of its own because a `vi.mock` factory can't close over `$state`
 * declared in the test file (the factory runs while the test's imports are still
 * resolving).
 */

let enabled = $state(true)
let showFileStatusIcons = $state(true)

export function getMediaIndexEnabled(): boolean {
  return enabled
}

export function getMediaIndexShowFileStatusIcons(): boolean {
  return showFileStatusIcons
}

export function setMediaIndexEnabled(value: boolean): void {
  enabled = value
}

export function setMediaIndexShowFileStatusIcons(value: boolean): void {
  showFileStatusIcons = value
}

/** Back to the both-on default. Call from `beforeEach`. */
export function resetMediaIndexSettings(): void {
  enabled = true
  showFileStatusIcons = true
}
