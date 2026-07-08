// Archive-password commands: store or clear the per-archive password used to
// unlock an encrypted archive (ZipCrypto today). The password lives on the
// resolved `ArchiveVolume` on the backend (remember-for-this-archive, LRU-capped,
// forgotten on restart or eviction). The frontend calls `setArchivePassword`
// after prompting, then retries the copy/move so the extract path can decrypt.
//
// Backend: `apps/desktop/src-tauri/src/commands/file_system/archive.rs`.

import { commands } from '$lib/ipc/bindings'

/**
 * Stores `password` for the archive at `archivePath` on `parentVolumeId`,
 * overwriting any previous one (so a fresh attempt replaces a rejected password).
 *
 * `archivePath` may be the archive file itself OR any path inside it — both
 * resolve to the same archive on the backend, so pass whichever is in hand (the
 * `.zip` path when prompting on a browse, an inner source path when prompting on
 * an extract).
 */
export async function setArchivePassword(parentVolumeId: string, archivePath: string, password: string): Promise<void> {
  const res = await commands.setArchivePassword(parentVolumeId, archivePath, password)
  if (res.status === 'error') throw new Error(res.error)
}

/**
 * Forgets any stored password for the archive (the user cancelled the prompt, or
 * the frontend is resetting state). A no-op when nothing was stored.
 */
export async function clearArchivePassword(parentVolumeId: string, archivePath: string): Promise<void> {
  const res = await commands.clearArchivePassword(parentVolumeId, archivePath)
  if (res.status === 'error') throw new Error(res.error)
}
