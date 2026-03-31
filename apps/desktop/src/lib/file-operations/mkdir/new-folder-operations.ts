import type { FilePaneAPI } from '$lib/file-explorer/pane/types'
import type { DirectoryDiff } from '$lib/file-explorer/types'
import { removeExtension } from './new-folder-utils'

type ListenFn = (event: string, handler: (event: { payload: DirectoryDiff }) => void) => Promise<() => void>
type FindFileIndexFn = (listingId: string, filename: string, showHiddenFiles: boolean) => Promise<number | null>

export async function getInitialFolderName(
  paneRef: FilePaneAPI | undefined,
  paneListingId: string,
  showHiddenFiles: boolean,
  getFileAt: (
    listingId: string,
    index: number,
    showHiddenFiles: boolean,
  ) => Promise<{ name: string; isDirectory: boolean } | null>,
): Promise<string> {
  try {
    const cursorIndex = paneRef?.getCursorIndex()
    const hasParent = paneRef?.hasParentEntry()
    if (cursorIndex === undefined || cursorIndex < 0) return ''
    const backendIndex = hasParent ? cursorIndex - 1 : cursorIndex
    if (backendIndex < 0) return ''
    const entry = await getFileAt(paneListingId, backendIndex, showHiddenFiles)
    if (!entry) return ''
    return entry.isDirectory ? entry.name : removeExtension(entry.name)
  } catch {
    return ''
  }
}

export async function moveCursorToNewFolder(
  paneListingId: string,
  folderName: string,
  paneRef: FilePaneAPI | undefined,
  hasParent: boolean,
  showHiddenFiles: boolean,
  listen: ListenFn,
  findFileIndex: FindFileIndexFn,
): Promise<void> {
  const unlisten = await listen('directory-diff', (event) => {
    if (event.payload.listingId !== paneListingId) return
    // Small delay to ensure listing cache is fully updated before querying
    setTimeout(() => {
      void findFileIndex(paneListingId, folderName, showHiddenFiles).then((index) => {
        if (index !== null) {
          const frontendIndex = hasParent ? index + 1 : index
          void paneRef?.setCursorIndex(frontendIndex)
          unlisten()
        }
      })
    }, 50)
  })
  // Clean up listener after 3 seconds if folder never appears
  setTimeout(() => {
    unlisten()
  }, 3000)
}
