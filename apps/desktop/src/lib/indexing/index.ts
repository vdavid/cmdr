/** Public API for the indexing module. */
export { isScanning, getEntriesScanned, getDirsFound, initIndexState, destroyIndexState } from './index-state.svelte'
export { initIndexEvents } from './index-events'
export { prioritizeDir, cancelNavPriority } from './index-priority'
