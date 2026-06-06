/** Public API for the indexing module. */
export {
  isScanning,
  getEntriesScanned,
  getDirsFound,
  getBytesScanned,
  getScanStartedAt,
  getPriorTotalEntries,
  getPriorScanDurationMs,
  getVolumeUsedBytes,
  isAggregating,
  getAggregationPhase,
  getAggregationCurrent,
  getAggregationTotal,
  getAggregationStartedAt,
  isReplaying,
  getReplayEventsProcessed,
  getReplayEstimatedTotal,
  getReplayStartedAt,
  initIndexState,
  destroyIndexState,
} from './index-state.svelte'
export { initIndexEvents } from './index-events'
