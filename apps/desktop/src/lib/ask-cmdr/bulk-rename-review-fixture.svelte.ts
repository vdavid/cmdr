/**
 * A real `$state` stand-in for the trigger's `askCmdrState`, for `BulkRenameReviewDialog`
 * tests. The dialog mints thumbnail tokens in an `$effect` keyed on the proposal id and drops
 * them when the review closes, so a test has to be able to open and close a review
 * reactively. A plain mock object can't: nothing re-runs the effect.
 */

import type { BulkRenameReview } from './ask-cmdr-trigger.svelte'

export const reviewState = $state<{ renameReview: BulkRenameReview | null }>({ renameReview: null })
