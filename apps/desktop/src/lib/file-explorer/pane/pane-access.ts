import type { FilePaneAPI } from './types'
import type { NavigationHistory } from '../navigation/navigation-history'
import type { SortColumn, SortOrder, VolumeInfo } from '../types'

/**
 * Read API over a dual-pane explorer's navigation + UI-chrome state.
 *
 * Command factories (clipboard, transfer, navigation, …) receive a `PaneAccess`
 * instead of reaching into `DualPaneExplorer`'s closures directly. It's the same
 * getter surface the explorer store will export later, so factories never change
 * signature when state moves from the component into a module store.
 *
 * Every getter returns a LIVE reference, never a copy or a `$state.snapshot`: a
 * call from inside a `$derived` / `$effect` must keep tracking the backing source
 * once that source becomes module `$state`. Returning a snapshot would silently
 * sever reactivity at the seam.
 */
export interface PaneAccess {
  getPaneRef: (pane: 'left' | 'right') => FilePaneAPI | undefined
  getPanePath: (pane: 'left' | 'right') => string
  getPaneVolumeId: (pane: 'left' | 'right') => string
  getPaneSort: (pane: 'left' | 'right') => { sortBy: SortColumn; sortOrder: SortOrder }
  getPaneHistory: (pane: 'left' | 'right') => NavigationHistory
  getFocusedPane: () => 'left' | 'right'
  otherPane: (pane: 'left' | 'right') => 'left' | 'right'
  getShowHiddenFiles: () => boolean
  getVolumes: () => VolumeInfo[]
  focusContainer: () => void
}
