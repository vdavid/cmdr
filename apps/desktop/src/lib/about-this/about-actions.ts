/**
 * The words for the card's footer actions. Which actions exist, their order, and which sit behind "More" are Rust's
 * (`cmdr_about::actions`); this only names them.
 */

import type { AboutAction } from '#lib/ipc/bindings.js'
import { tString } from '#lib/intl/messages.svelte.js'

/** The button's (or the "More" row's) label. */
export function actionLabel(action: AboutAction): string {
  switch (action.type) {
    case 'openWithDefaultApp':
      return tString('aboutThis.action.open')
    case 'showInFileManager':
      return tString('aboutThis.action.showInFileManager')
    case 'openDownloadPage':
      return tString('aboutThis.action.downloadPage')
    case 'revealOwner':
      return tString('aboutThis.action.revealOwner', { app: action.app })
    case 'goToAppFolder':
      return action.app === null
        ? tString('aboutThis.action.appFolderUnnamed')
        : tString('aboutThis.action.appFolder', { app: action.app })
    case 'moveToTrash':
      return tString('aboutThis.action.trash')
    case 'delete':
      return tString('aboutThis.action.delete')
  }
}

/**
 * The tooltip that says where a press goes before it happens (on hover and on keyboard focus): the site a download
 * page opens, and the app Open opens with.
 */
export function actionTooltip(action: AboutAction): string | null {
  switch (action.type) {
    case 'openDownloadPage':
      return tString('aboutThis.action.downloadPageTooltip', { host: action.host })
    case 'openWithDefaultApp':
      return tString('aboutThis.action.openTooltip', { app: action.app })
    default:
      return null
  }
}
