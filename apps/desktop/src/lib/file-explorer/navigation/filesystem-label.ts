/**
 * Display label for a volume's filesystem, shown in the volume picker (the
 * current-volume tooltip and each dropdown item).
 *
 * Filesystem format names (APFS, FAT32, exFAT, NTFS, ...) are proper nouns, not
 * localized — like brand names — so they live here as literals rather than in
 * the i18n catalog. The raw → label classification mirrors the Rust
 * `FilesystemKind` (src-tauri `file_system/filesystem_kind.rs`), the source of
 * truth the copy/move size guard reads; keep the two in sync.
 *
 * Only REAL local filesystems get a label: disk images, cloud drives,
 * favorites, mobile (MTP) devices, and OS-mounted SMB shares (whose backing
 * filesystem we can't see) return `null` and show nothing.
 *
 * A NETWORK row answers with the PROTOCOL it speaks instead ("SFTP", "WebDAV",
 * "SMB"): the slot's question is "what am I talking to", and for a place with no
 * local mount the protocol is the only honest answer. Protocol names are proper
 * nouns too, so they live here beside the filesystem ones.
 */
import type { VolumeInfo } from '../types'

/**
 * Raw OS filesystem-type string (macOS `statfs.f_fstypename`, Linux
 * `/proc/mounts`) → display name. macOS reports FAT16/FAT32 alike as `msdos`
 * and FAT32 is the common case, so `msdos`/`vfat` show as "FAT32" (matching the
 * size-guard dialog). Network (`smbfs`/`cifs`) is intentionally absent — we
 * can't see an SMB share's real backing filesystem.
 */
const FS_LABELS: Record<string, string> = {
  apfs: 'APFS',
  hfs: 'HFS+',
  hfsplus: 'HFS+',
  'hfs+': 'HFS+',
  ext4: 'ext4',
  ext3: 'ext3',
  ext2: 'ext2',
  btrfs: 'Btrfs',
  xfs: 'XFS',
  zfs: 'ZFS',
  ntfs: 'NTFS',
  ntfs3: 'NTFS',
  ufsd_ntfs: 'NTFS',
  exfat: 'exFAT',
  'fuse.exfat': 'exFAT',
  msdos: 'FAT32',
  vfat: 'FAT32',
  fat: 'FAT32',
  fat32: 'FAT32',
  fat16: 'FAT16',
}

/**
 * `fsType` → protocol name, for a row in the Network group. The two SMB
 * spellings are the OS's (macOS `smbfs`, Linux `cifs`); `sftp` and `webdav` are
 * what the servers listing arm publishes (`src-tauri/src/server_volumes.rs`).
 */
const PROTOCOL_LABELS: Record<string, string> = {
  sftp: 'SFTP',
  webdav: 'WebDAV',
  smbfs: 'SMB',
  cifs: 'SMB',
}

/**
 * The protocol name for an `fsType`, or `null` when it isn't one of ours.
 *
 * Exported because the servers hub's Type column asks the same question about a
 * row that isn't a `VolumeInfo` yet; one map means the switcher and the hub can't
 * disagree about what to call SFTP.
 */
export function protocolLabel(fsType: string): string | null {
  return PROTOCOL_LABELS[fsType.toLowerCase()] ?? null
}

/** Whether a volume represents a real local filesystem worth labeling. */
function isRealFilesystemVolume(volume: VolumeInfo): boolean {
  if (volume.isDiskImage) return false
  return volume.category === 'main_volume' || volume.category === 'attached_volume'
}

/**
 * The display label for a volume's filesystem, or `null` when there's nothing
 * meaningful to show (not a real local filesystem, or an unrecognized type).
 */
export function filesystemLabel(volume: VolumeInfo): string | null {
  const raw = volume.fsType?.toLowerCase()
  if (!raw) return null
  if (volume.category === 'network') return protocolLabel(raw)
  if (!isRealFilesystemVolume(volume)) return null
  return FS_LABELS[raw] ?? null
}
