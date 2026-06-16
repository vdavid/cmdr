import { tString } from '$lib/intl/messages.svelte'
import type { MessageKey } from '$lib/intl/keys.gen'
import type { VolumeInfo, LocationCategory } from '../types'

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

export interface GroupingOptions {
  /** When false, the synthetic "Network" entry shows as "Network (disabled)" and clicking it opens settings instead of navigating. */
  networkEnabled: boolean
}

export function groupByCategory(
  vols: VolumeInfo[],
  options: GroupingOptions = { networkEnabled: true },
): VolumeGroup[] {
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
      // Network section: show a single "Network" item that opens NetworkBrowser
      // Also include any pre-mounted network volumes (mounted shares).
      // When networking is disabled in Settings, the synthetic entry is labelled
      // "Network (disabled)": already-mounted shares stay listed (filesystem I/O on
      // them doesn't need Local Network permission).
      const networkVolumes = vols.filter((v) => v.category === 'network')

      const networkItem: VolumeInfo = {
        id: 'network',
        name: options.networkEnabled
          ? tString('fileExplorer.navigation.networkVolume')
          : tString('fileExplorer.navigation.networkVolumeDisabled'),
        path: 'smb://', // Virtual path
        category: 'network' as const,
        icon: undefined, // Will use placeholder
        isEjectable: false,
      }

      const allItems = [networkItem, ...networkVolumes]
      groups.push({ category, label, items: allItems })
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
