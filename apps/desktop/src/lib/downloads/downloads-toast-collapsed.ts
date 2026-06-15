/**
 * Reader/writer for the hidden `behavior.fileSystemWatching.downloadsToastCollapsed`
 * setting. The downloads toast remembers whether the user last collapsed it, so a
 * NEW toast opens in the same state. There's no Settings UI for this flag; it's
 * driven entirely by the toast's collapse/expand button.
 */
import { getSetting, setSetting } from '$lib/settings'

const COLLAPSED_KEY = 'behavior.fileSystemWatching.downloadsToastCollapsed'

export function getDownloadsToastCollapsed(): boolean {
  return getSetting(COLLAPSED_KEY)
}

export function setDownloadsToastCollapsed(value: boolean): void {
  setSetting(COLLAPSED_KEY, value)
}
