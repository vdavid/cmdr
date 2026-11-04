import { describe, expect, it } from 'vitest'
import { writeAnswers } from './writes.js'

/** Runs one write and returns what it threw (every write must throw), or fails the test. */
function refusal(name: keyof typeof writeAnswers, payload: Record<string, unknown>): unknown {
  const answer = writeAnswers[name] as ((p: Record<string, unknown>) => unknown) | undefined
  if (!answer) throw new Error(`no answer for ${name}`)
  try {
    answer(payload)
  } catch (error) {
    return error
  }
  throw new Error(`${name} succeeded, but the demo writes nothing`)
}

describe('write answers', () => {
  it.each([
    ['createDirectory', { parentPath: '/Users/you', name: 'New' }, '/Users/you'],
    ['createFile', { parentPath: '/Users/you/Documents', name: 'a.txt' }, '/Users/you/Documents'],
    ['renameFile', { from: '/Users/you/notes.md', to: '/Users/you/n.md' }, '/Users/you'],
    ['checkRenamePermission', { path: '/Users/you/notes.md' }, '/Users/you'],
    ['moveToTrash', { path: '/notes.md' }, '/'],
    ['pasteClipboardAsFile', { directory: '/Users/you/Desktop' }, '/Users/you/Desktop'],
  ] as const)('%s refuses: the folder takes no writes', (name, payload, folder) => {
    expect(refusal(name, payload)).toEqual({ type: 'parentNotWritable', path: folder })
  })

  it.each([
    ['moveFiles', { sources: ['/Users/you/a', '/Users/you/b'], destination: '/x' }, '/Users/you/a', 'source'],
    ['deleteFiles', { sources: ['/Users/you/a'] }, '/Users/you/a', 'source'],
    ['trashFiles', { sources: [] }, '/', 'source'],
    [
      'copyBetweenVolumes',
      { sourcePaths: ['/a'], destPath: '/Users/you/Desktop' },
      '/Users/you/Desktop',
      'destination',
    ],
    ['moveBetweenVolumes', { sourcePaths: ['/Users/you/a'], destPath: '/x' }, '/Users/you/a', 'source'],
  ] as const)('%s refuses: the disk is read-only', (name, payload, path, side) => {
    expect(refusal(name, payload)).toEqual({ type: 'read_only_device', path, deviceName: 'Macintosh HD', side })
  })

  it('answers every write it lists with a refusal, never a success', () => {
    for (const name of Object.keys(writeAnswers) as (keyof typeof writeAnswers)[]) {
      expect(
        refusal(name, { sources: ['/a'], sourcePaths: ['/a'], path: '/a', from: '/a', parentPath: '/' }),
      ).toBeTypeOf('object')
    }
  })
})
