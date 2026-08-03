/** Public API for the indexing module. */
export {
  ROOT_VOLUME_ID,
  isVolumeScanning,
  getEntriesScanned,
  getVolumeActivity,
  getVolumeAggregation,
  getVolumePhase,
  placeholderActivity,
  initIndexState,
  destroyIndexState,
} from './index-state.svelte'
export type { VolumeIndexActivity } from './index-state.svelte'
export { initMediaEnrichState, destroyMediaEnrichState, getEnrichingVolumes } from './media-enrich-state.svelte'
export { initIndexEvents } from './index-events'
