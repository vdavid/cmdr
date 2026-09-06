import { tString } from '$lib/intl/messages.svelte'
import type { MessageKey } from '$lib/intl/keys.gen'
import type { VolumeInfo, LocationCategory } from '../types'
import { hasReconnectLoop, isLiveSession } from './connection-state'

export interface VolumeGroup {
  category: LocationCategory
  label: string
  items: VolumeInfo[]
}

// Labels are resolved lazily (per call) so they track the active locale; the
// caller invokes `groupByCategory` from a reactive `$derived`.
const categoryOrder: { category: LocationCategory; labelKey: MessageKey | null }[] = [
  { category: 'favorite', labelKey: 'fileExplorer.navigation.groupFavorites' },
  { category: 'main_volume', labelKey: 'fileExplorer.navigation.groupVolumes' },
  { category: 'attached_volume', labelKey: null }, // No label, continues main volumes
  { category: 'cloud_drive', labelKey: 'fileExplorer.navigation.groupCloud' },
  { category: 'mobile_device', labelKey: 'fileExplorer.navigation.groupMobile' },
  { category: 'network', labelKey: 'fileExplorer.navigation.groupNetwork' },
]

export function groupByCategory(vols: VolumeInfo[]): VolumeGroup[] {
  const groups: VolumeGroup[] = []

  for (const { category, labelKey } of categoryOrder) {
    const label = labelKey ? tString(labelKey) : ''
    if (category === 'favorite') {
      // The Favorites group always renders, even when empty: an emptied list is a real
      // user state (they can remove every favorite), and the switcher shows a disabled
      // "(Your favorites will show here)" placeholder for it. Every other group hides when
      // empty.
      const items = vols.filter((v) => v.category === 'favorite')
      groups.push({ category, label, items })
    } else if (category === 'mobile_device') {
      const mobileItems = vols.filter((v) => v.category === 'mobile_device')
      if (mobileItems.length > 0) {
        groups.push({ category, label, items: mobileItems })
      }
    } else if (category === 'network') {
      // ❗ The hub row is here whatever `network.enabled` says. That switch gates
      // mDNS discovery and SMB, which is what the macOS Local Network permission
      // is about; SFTP and WebDAV need none of it, and the hub says so in its own
      // list rather than by refusing to open.
      const networkVolumes = vols.filter((v) => v.category === 'network' && belongsInSwitcher(v))

      const hubRow: VolumeInfo = {
        id: 'network',
        name: tString('fileExplorer.navigation.networkVolume'),
        path: 'smb://', // Virtual path
        category: 'network' as const,
        icon: undefined, // Will use placeholder
        isEjectable: false,
      }

      groups.push({ category, label, items: [hubRow, ...networkVolumes] })
    } else {
      const items = vols.filter((v) => v.category === category)
      if (items.length > 0) {
        // Merge attached_volume into the previous group (main_volume)
        if (category === 'attached_volume' && groups.length > 0) {
          const lastGroup = groups[groups.length - 1]
          if (lastGroup.category === 'main_volume') {
            lastGroup.items.push(...items)
            continue
          }
        }
        groups.push({ category, label, items })
      }
    }
  }

  return groups
}

/**
 * Whether a Network row earns a place in the switcher: the three-things rule,
 * minus the hub row, which `groupByCategory` synthesizes.
 *
 * A place shows when a session stands behind it (live, or being recovered) or
 * when the user pinned it. ❗ Only a server place carries a pin at all: a mounted
 * SMB share was never subject to the cap, and on Linux it carries no connection
 * state either, so a rule written as "live or pinned" would drop every CIFS
 * mount off the switcher.
 *
 * The listing itself hides nothing (`server_volumes.rs::append_server_volumes`):
 * an unpinned server still has to be an id the hub, a restored tab, and a
 * favorite can all resolve.
 */
function belongsInSwitcher(volume: VolumeInfo): boolean {
  if (volume.pinned == null) return true
  return volume.pinned || isLiveSession(volume.connectionState) || hasReconnectLoop(volume.connectionState)
}

export function getIconForVolume(volume: VolumeInfo | undefined): string | undefined {
  if (!volume) return undefined
  if (volume.category === 'cloud_drive') {
    return '/icons/sync-online-only.svg'
  }
  if (volume.category === 'mobile_device') {
    return '/icons/mobile-device.svg'
  }
  if (volume.category === 'network' && !volume.icon) {
    return undefined // Will use placeholder
  }
  return volume.icon
}
