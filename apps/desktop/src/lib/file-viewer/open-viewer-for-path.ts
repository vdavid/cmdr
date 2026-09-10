import { resolvePathVolume } from '$lib/tauri-commands'
import { openFileViewer } from './open-viewer'

/**
 * Opens the viewer for a path that arrived without its volume: an MCP
 * `dialog open file-viewer` call names only a path.
 *
 * ❗ Asks the backend which volume holds the path, ❌ never assumes `root`: the
 * viewer opens a phone's or server's file against the volume it's given, and
 * against `root` it answers `notFound`. Resolving never dials (a phone answers
 * from its cached device row, a server from its saved row), and a path nothing
 * claims opens on the local drive as before.
 */
export async function openFileViewerForPath(filePath: string): Promise<void> {
  const { volume } = await resolvePathVolume(filePath)
  await openFileViewer(filePath, volume?.id ?? 'root')
}
