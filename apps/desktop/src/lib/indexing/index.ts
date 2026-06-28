/** Public API for the indexing module. */
export {
  isScanning,
  getEntriesScanned,
  getVolumeActivity,
  getVolumeAggregation,
  initIndexState,
  destroyIndexState,
} from './index-state.svelte'
export type { VolumeIndexActivity, AggregationActivity } from './index-state.svelte'
export { initIndexEvents } from './index-events'
