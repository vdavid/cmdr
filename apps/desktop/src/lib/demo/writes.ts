/**
 * The demo's answers for every write: none lands. The demo volume reports `mountIsReadOnly`
 * (`volumes.ts`), so the app refuses most writes with its own read-only alert before it asks; any
 * that still reaches the backend (a drop, a paste, an agent) gets the typed refusal a read-only disk
 * gives, never a pretend success. Real file operations in the demo are #929.
 */
import { refuse, type AnswerTable, type Payload } from './answer-types.js'

/** The folder holding `path`: what a refusal to write there names. */
function parentOf(path: string): string {
  const cut = path.lastIndexOf('/')
  return cut > 0 ? path.slice(0, cut) : '/'
}

/** The transfer refusal for a read-only disk, in the app's own wording ("… is read-only"). */
function readOnly(path: string, side: 'source' | 'destination') {
  return { type: 'read_only_device', path, deviceName: 'Macintosh HD', side } as const
}

function first(paths: unknown): string {
  return (paths as string[] | undefined)?.[0] ?? '/'
}

const notWritable = (path: string) => ({ type: 'parentNotWritable', path }) as const

export const writeAnswers: AnswerTable = {
  createDirectory: (p: Payload) => refuse('createDirectory', notWritable(p.parentPath as string)),
  createFile: (p) => refuse('createFile', notWritable(p.parentPath as string)),
  renameFile: (p) => refuse('renameFile', notWritable(parentOf(p.from as string))),
  checkRenamePermission: (p) => refuse('checkRenamePermission', notWritable(parentOf(p.path as string))),
  moveToTrash: (p) => refuse('moveToTrash', notWritable(parentOf(p.path as string))),
  pasteClipboardAsFile: (p) => refuse('pasteClipboardAsFile', notWritable(p.directory as string)),
  copyBetweenVolumes: (p) => refuse('copyBetweenVolumes', readOnly(p.destPath as string, 'destination')),
  moveBetweenVolumes: (p) => refuse('moveBetweenVolumes', readOnly(first(p.sourcePaths), 'source')),
  moveFiles: (p) => refuse('moveFiles', readOnly(first(p.sources), 'source')),
  deleteFiles: (p) => refuse('deleteFiles', readOnly(first(p.sources), 'source')),
  trashFiles: (p) => refuse('trashFiles', readOnly(first(p.sources), 'source')),
}
