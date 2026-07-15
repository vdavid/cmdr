/** Public API for the indexing module. */
export {
  ROOT_VOLUME_ID,
  isVolumeScanning,
  isVolumeAggregating,
  getEntriesScanned,
  getVolumeActivity,
  getVolumeAggregation,
  getVolumePhase,
  placeholderActivity,
  initIndexState,
  destroyIndexState,
} from './index-state.svelte'
export type { VolumeIndexActivity, AggregationActivity } from './index-state.svelte'
export { initMediaEnrichState, destroyMediaEnrichState } from './media-enrich-state.svelte'
export type { VolumeEnrichActivity } from './media-enrich-state.svelte'
export { initIndexEvents } from './index-events'
