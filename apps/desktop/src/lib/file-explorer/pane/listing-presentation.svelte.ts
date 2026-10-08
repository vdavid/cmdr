/**
 * Keeps a pane's last settled listing on screen during a short navigation.
 * Fast directory loads never replace useful rows with a transient loading frame;
 * a load that lasts 100 ms earns the normal loading screen.
 */

export const LISTING_LOADING_DELAY_MS = 100

export interface ListingPresentationDeps {
  getListingId: () => string
  getTotalCount: () => number
  getLoading: () => boolean
  /** The `..` row the CURRENT path asks for, which moves before its listing lands. */
  getParentRow: () => ParentRow
}

/** Whether a list starts with the synthetic `..` row, and where that row points. */
export interface ParentRow {
  hasParent: boolean
  parentPath: string
}

export interface ListingPresentation {
  /** Listing id the list view should render while a load is in flight. */
  readonly listingId: string
  /** Row count paired with `listingId`. */
  readonly totalCount: number
  /**
   * The `..` row paired with `listingId`. It is part of that listing: the current
   * path's `..` over the previous rows can repeat one of them (`/a` → `/a/b/c` makes
   * it `/a/b`), a duplicate key in the list's keyed `#each`.
   */
  readonly parentRow: ParentRow
  /** Whether the loading screen has outlasted the grace period. */
  readonly showLoading: boolean
}

export function createListingPresentation(deps: ListingPresentationDeps): ListingPresentation {
  let settledListingId = $state('')
  let settledTotalCount = $state(0)
  let settledParentRow = $state<ParentRow>({ hasParent: false, parentPath: '' })
  let showLoading = $state(deps.getLoading())

  $effect(() => {
    const loading = deps.getLoading()
    let delayTimer: ReturnType<typeof setTimeout> | undefined

    if (!loading) {
      const listingId = deps.getListingId()
      if (listingId) {
        settledListingId = listingId
        settledTotalCount = deps.getTotalCount()
        settledParentRow = deps.getParentRow()
      }
      showLoading = false
    } else if (!settledListingId) {
      // Startup has no useful rows to preserve.
      showLoading = true
    } else {
      showLoading = false
      delayTimer = setTimeout(() => {
        showLoading = true
      }, LISTING_LOADING_DELAY_MS)
    }

    return () => {
      if (delayTimer !== undefined) clearTimeout(delayTimer)
    }
  })

  return {
    get listingId() {
      return deps.getLoading() ? settledListingId : deps.getListingId()
    },
    get totalCount() {
      return deps.getLoading() ? settledTotalCount : deps.getTotalCount()
    },
    get parentRow() {
      return deps.getLoading() ? settledParentRow : deps.getParentRow()
    },
    get showLoading() {
      return showLoading
    },
  }
}
