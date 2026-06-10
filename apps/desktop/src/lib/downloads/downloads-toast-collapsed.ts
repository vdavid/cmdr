/**
 * Reader/writer for the hidden `behavior.fileSystemWatching.downloadsToastCollapsed`
 * setting. The downloads toast remembers whether the user last collapsed it, so a
 * NEW toast opens in the same state. There's no Settings UI for this flag; it's
 * driven entirely by the toast's collapse/expand button.
 */
import { getSetting, setSetting } from '$lib/settings'

const COLLAPSED_KEY = 'behavior.fileSystemWatching.downloadsToastCollapsed'

export function getDownloadsToastCollapsed(): boolean {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- key is in the registry
  return getSetting(COLLAPSED_KEY as any) as boolean
}

export function setDownloadsToastCollapsed(value: boolean): void {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- key is in the registry
  setSetting(COLLAPSED_KEY as any, value)
}
