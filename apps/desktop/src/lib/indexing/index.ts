/** Public API for the indexing module. */
export {
    isScanning,
    getEntriesScanned,
    getDirsFound,
    isAggregating,
    getAggregationPhase,
    getAggregationCurrent,
    getAggregationTotal,
    getAggregationStartedAt,
    initIndexState,
    destroyIndexState,
} from './index-state.svelte'
export { initIndexEvents } from './index-events'
