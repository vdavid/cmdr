import type { VolumeInfo, LocationCategory } from '../types'
import type { MtpVolume } from '$lib/mtp'

export interface VolumeGroup {
  category: LocationCategory
  label: string
  items: VolumeInfo[]
}

const categoryOrder: { category: LocationCategory; label: string }[] = [
  { category: 'favorite', label: 'Favorites' },
  { category: 'main_volume', label: 'Volumes' },
  { category: 'attached_volume', label: '' }, // No label, continues main volumes
  { category: 'cloud_drive', label: 'Cloud' },
  { category: 'mobile_device', label: 'Mobile' },
  { category: 'network', label: 'Network' },
]

export function groupByCategory(vols: VolumeInfo[], mtpVols: MtpVolume[]): VolumeGroup[] {
  const groups: VolumeGroup[] = []

  for (const { category, label } of categoryOrder) {
    if (category === 'mobile_device') {
      // Mobile section: show MTP volumes (one per storage on connected devices)
      const mobileItems: VolumeInfo[] = mtpVols.map((v) => ({
        id: v.id,
        name: v.name,
        path: v.path,
        category: 'mobile_device' as const,
        icon: undefined, // Will use placeholder
        isEjectable: true,
        isReadOnly: v.isReadOnly,
      }))

      if (mobileItems.length > 0) {
        groups.push({ category, label, items: mobileItems })
      }
    } else if (category === 'network') {
      // Network section: show a single "Network" item that opens NetworkBrowser
      // Also include any pre-mounted network volumes (mounted shares)
      const networkVolumes = vols.filter((v) => v.category === 'network')

      // Create the single "Network" entry that opens NetworkBrowser
      const networkItem: VolumeInfo = {
        id: 'network',
        name: 'Network',
        path: 'smb://', // Virtual path
        category: 'network' as const,
        icon: undefined, // Will use placeholder
        isEjectable: false,
      }

      // Show network entry plus any mounted shares
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
