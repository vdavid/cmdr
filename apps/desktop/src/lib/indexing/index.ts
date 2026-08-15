/** Public API for the indexing module. */
export {
  ROOT_VOLUME_ID,
  isVolumeScanning,
  getEntriesScanned,
  getVolumeActivity,
  getVolumeAggregation,
  getVolumePhase,
  getWalkedGround,
  placeholderActivity,
  initIndexState,
  destroyIndexState,
} from './index-state.svelte'
export type { VolumeIndexActivity } from './index-state.svelte'
export { isPathAffectedByWalk, NO_WALKED_GROUND } from './walked-ground'
export type { WalkedGround } from './walked-ground'
export { initMediaEnrichState, destroyMediaEnrichState, getEnrichingVolumes } from './media-enrich-state.svelte'
export { initIndexEvents } from './index-events'
