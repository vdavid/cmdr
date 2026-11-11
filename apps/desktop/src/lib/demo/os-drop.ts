/**
 * Files dragged in from the visitor's computer. On desktop, a Finder drag reaches the webview as
 * wry's drag-drop events with real paths, and the app's drag controller opens the transfer dialog.
 * A browser hands over `File` objects with no path, so this lands their bytes on the in-memory disk
 * first (a staging folder, `crates/cmdr-listing-wasm/src/imports.rs`), then sends the same events
 * with the staged paths. The controller (`file-explorer/pane/drag-drop-controller.svelte.ts`) runs
 * unchanged: its hit test, the transfer dialog, its conflict handling, and the write engine.
 * Nothing leaves the page. See `DETAILS.md` § Files dropped in from the computer.
 */

/** Sends one event to the main window, as wry does for its webview. */
type EmitToMain = (event: string, payload: unknown) => void

/** One dropped item, as the engine plans it (`ImportItem` in `imports.rs`). */
export interface ImportItem {
  /** Relative to the drop, `/`-separated; a folder comes before everything in it. */
  path: string
  kind: 'file' | 'folder'
  size: number
  modifiedMs?: number
}

/** Why some of a drop stayed out (`ImportRefusal` in `imports.rs`). */
export interface ImportRefusal {
  kind: 'overBudget'
  budgetBytes: number
  leftOut: number
}

/** The engine's plan for a drop (`ImportPlan` in `imports.rs`). */
export interface ImportPlan {
  batch: number
  accepted: number
  roots: string[]
  refusal: ImportRefusal | null
}

/** The engine's import door. */
export interface DropImporter {
  plan(items: readonly ImportItem[]): Promise<ImportPlan>
  file(batch: number, index: number, bytes: Uint8Array): Promise<unknown>
  /** Done landing `batch`: `offered` when its paths went to the app as a drop, else it's swept at once. */
  handOff(batch: number, offered: boolean): Promise<unknown>
  /** No dialog holds file operations any more: sweeps every drop no transfer claimed. */
  releaseOffered(): Promise<unknown>
}

/** What a drop read off the browser: the engine's item, plus the file to read its bytes from. */
export interface DroppedItem extends ImportItem {
  file?: File
}

/** The subset of `DataTransferItem` a drop reads. */
interface DropItem {
  kind: string
  getAsFile(): File | null
  webkitGetAsEntry?: () => FileSystemEntry | null
}

/** The subset of `DataTransfer` a drop reads. */
export interface DropData {
  items?: ArrayLike<DropItem>
  files: ArrayLike<File>
}

/**
 * Where the drag says it comes from while it's over the page: the browser shows no names until
 * the drop. A path on the demo disk, so the controller's Move-or-Copy verdict while hovering is
 * the one the staged paths get.
 */
export const INCOMING_PATH = '/.drops/incoming'

/**
 * The size the drag image reports. The browser draws the OS's own drag image (Finder's preview)
 * over the page, so the app's overlay stays hidden, as it does for a Finder drag on desktop.
 */
const OS_DRAG_IMAGE = { width: 512, height: 512 }

function fileItem(path: string, file: File): DroppedItem {
  return { path, kind: 'file', size: file.size, modifiedMs: file.lastModified || undefined, file }
}

function fileOf(entry: FileSystemFileEntry): Promise<File> {
  return new Promise((resolve, reject) => {
    entry.file(resolve, reject)
  })
}

/** Every child of `dir`: `readEntries` hands them over in batches until an empty one. */
async function childrenOf(dir: FileSystemDirectoryEntry): Promise<FileSystemEntry[]> {
  const reader = dir.createReader()
  const children: FileSystemEntry[] = []
  for (;;) {
    const batch = await new Promise<FileSystemEntry[]>((resolve, reject) => {
      reader.readEntries(resolve, reject)
    })
    if (batch.length === 0) break
    children.push(...batch)
  }
  return children.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
}

async function walk(entry: FileSystemEntry, path: string, into: DroppedItem[]): Promise<void> {
  if (entry.isDirectory) {
    into.push({ path, kind: 'folder', size: 0 })
    for (const child of await childrenOf(entry as FileSystemDirectoryEntry)) {
      // eslint-disable-next-line cmdr/no-raw-path-arithmetic -- a dropped folder's tree in the browser demo, `/`-separated by the File System API
      await walk(child, `${path}/${child.name}`, into)
    }
  } else if (entry.isFile) {
    into.push(fileItem(path, await fileOf(entry as FileSystemFileEntry)))
  }
}

/**
 * The drop's items, folders first and in structure, through `webkitGetAsEntry` where the browser
 * has it; else its flat files. Reads the entries at once: a `DataTransfer` empties after the
 * `drop` handler returns.
 */
export async function readDrop(data: DropData): Promise<DroppedItem[]> {
  const roots: (FileSystemEntry | File)[] = []
  const items = data.items ? Array.from(data.items) : []
  for (const item of items) {
    if (item.kind !== 'file') continue
    const entry = item.webkitGetAsEntry?.() ?? null
    const file = entry ? null : item.getAsFile()
    if (entry) roots.push(entry)
    else if (file) roots.push(file)
  }
  if (roots.length === 0) roots.push(...Array.from(data.files))

  const dropped: DroppedItem[] = []
  for (const root of roots) {
    if (root instanceof File) dropped.push(fileItem(root.name, root))
    else await walk(root, root.name, dropped)
  }
  return dropped
}

/**
 * Lands `dropped` on the disk: the engine's plan, then each accepted file's bytes. Answers the
 * staged top-level paths that landed (a file that didn't is left out) and the plan's refusal.
 */
export async function importDropped(
  importer: DropImporter,
  dropped: readonly DroppedItem[],
): Promise<{ batch: number; paths: string[]; refusal: ImportRefusal | null }> {
  const plan = await importer.plan(
    dropped.map(({ path, kind, size, modifiedMs }) => ({ path, kind, size, modifiedMs })),
  )
  const missing = new Set<string>()
  for (let index = 0; index < plan.accepted; index++) {
    const { file, path } = dropped[index]
    if (!file) continue
    try {
      await importer.file(plan.batch, index, new Uint8Array(await file.arrayBuffer()))
    } catch (refusal) {
      // A file that changed or went away mid-drop: its top-level item stays out of the drop.
      // eslint-disable-next-line no-console -- the demo's developer reads the devtools console
      console.warn('[demo] a dropped file did not land', refusal)
      if (!path.includes('/')) missing.add(path)
    }
  }
  const paths = plan.roots.filter((root) => !missing.has(root.slice(root.lastIndexOf('/') + 1)))
  return { batch: plan.batch, paths, refusal: plan.refusal }
}

/** Whether a browser drag carries files from outside the page. */
function carriesFiles(event: DragEvent): boolean {
  return event.dataTransfer?.types.includes('Files') ?? false
}

function modifiersOf(event: DragEvent): { altHeld: boolean; cmdHeld: boolean; shiftHeld: boolean } {
  return { altHeld: event.altKey, cmdHeld: event.metaKey, shiftHeld: event.shiftKey }
}

export interface OsDrops {
  /** Removes every listener. */
  dispose(): void
}

/**
 * Answers the browser's drag events for files from outside the page with the drag-drop events the
 * app hears from wry. `onRefusal` hears a drop the budget cut short, after the rest landed.
 */
export function createOsDrops(
  win: Window,
  emit: EmitToMain,
  importer: DropImporter,
  onRefusal: (refusal: ImportRefusal) => void,
): OsDrops {
  // `dragenter` / `dragleave` fire per element; the drag is over the page while the count is positive.
  let depth = 0
  let sentModifiers = ''

  function sendModifiers(event: DragEvent): void {
    const modifiers = modifiersOf(event)
    const key = JSON.stringify(modifiers)
    if (key === sentModifiers) return
    sentModifiers = key
    emit('drag-modifiers', modifiers)
  }

  function position(event: DragEvent): { x: number; y: number } {
    return { x: event.clientX, y: event.clientY }
  }

  function leave(): void {
    depth = 0
    sentModifiers = ''
    emit('tauri://drag-leave', null)
  }

  function onDragEnter(event: DragEvent): void {
    if (!carriesFiles(event)) return
    event.preventDefault()
    depth += 1
    if (depth > 1) return
    // The swizzle's order: the image size and the keys first, so the first verdict has them.
    emit('drag-image-size', OS_DRAG_IMAGE)
    sendModifiers(event)
    emit('tauri://drag-enter', { paths: [INCOMING_PATH], position: position(event) })
  }

  function onDragOver(event: DragEvent): void {
    // Without this the browser opens a dropped file, or follows a dropped link, in place of the demo.
    event.preventDefault()
    if (!carriesFiles(event) || depth === 0) return
    sendModifiers(event)
    emit('tauri://drag-over', { position: position(event) })
  }

  function onDragLeave(event: DragEvent): void {
    if (!carriesFiles(event) || depth === 0) return
    depth -= 1
    if (depth === 0) leave()
  }

  function onDrop(event: DragEvent): void {
    event.preventDefault()
    if (!carriesFiles(event) || !event.dataTransfer) return
    const at = position(event)
    const wasInside = depth > 0
    depth = 0
    sentModifiers = ''
    // Read now: the `DataTransfer` empties once this handler returns.
    const reading = readDrop(event.dataTransfer)
    void (async () => {
      try {
        const { batch, paths, refusal } = await importDropped(importer, await reading)
        if (refusal) onRefusal(refusal)
        // Before the drop goes out, so a transfer that starts at once finds the batch claimable.
        await importer.handOff(batch, paths.length > 0)
        if (paths.length === 0) {
          if (wasInside) emit('tauri://drag-leave', null)
          return
        }
        if (!wasInside) {
          emit('drag-image-size', OS_DRAG_IMAGE)
          emit('tauri://drag-enter', { paths, position: at })
        }
        emit('tauri://drag-drop', { paths, position: at })
      } catch (refusal) {
        // eslint-disable-next-line no-console -- the demo's developer reads the devtools console
        console.warn('[demo] a drop from the computer did not land', refusal)
        if (wasInside) emit('tauri://drag-leave', null)
      }
    })()
  }

  const listeners: [string, (event: DragEvent) => void][] = [
    ['dragenter', onDragEnter],
    ['dragover', onDragOver],
    ['dragleave', onDragLeave],
    ['drop', onDrop],
  ]
  for (const [type, listener] of listeners) win.addEventListener(type, listener as EventListener)

  return {
    dispose() {
      for (const [type, listener] of listeners) win.removeEventListener(type, listener as EventListener)
    },
  }
}
