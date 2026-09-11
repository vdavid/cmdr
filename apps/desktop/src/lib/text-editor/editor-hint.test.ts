/**
 * The one-time text editor hint, one test per rule. Pure: no IPC, no store, no
 * toast. `open-file-in-editor.test.ts` pins what the caller does with the answer.
 */

import { describe, it, expect } from 'vitest'
import type { EditorOpenReport } from '$lib/ipc/bindings'
import { decideEditorHint } from './editor-hint'

const SYSTEM = 'system'

function report(otherEditorsInstalled: boolean | null, openedInName: string | null = 'TextEdit'): EditorOpenReport {
  return { outcome: 'opened', openedInName, otherEditorsInstalled }
}

describe('decideEditorHint', () => {
  it('says nothing and writes nothing once the hint is spent', () => {
    expect(decideEditorHint({ hintSeen: true, storedChoice: SYSTEM, report: report(true) })).toEqual({
      showHint: false,
      markSeen: false,
    })
  })

  it('says nothing and leaves the flag unspent when the launch threw', () => {
    expect(decideEditorHint({ hintSeen: false, storedChoice: SYSTEM, report: null })).toEqual({
      showHint: false,
      markSeen: false,
    })
  })

  it('spends the flag silently when the user already chose an editor', () => {
    // They found the setting on their own, and Rust wasn't asked about other editors.
    expect(decideEditorHint({ hintSeen: false, storedChoice: 'com.sublimetext.4', report: report(null) })).toEqual({
      showHint: false,
      markSeen: true,
    })
  })

  it('says nothing and writes nothing when nobody could answer (Linux, or not asked)', () => {
    // The Linux guarantee: `xdg-open` never reports other editors, so the hint can't fire there.
    expect(decideEditorHint({ hintSeen: false, storedChoice: SYSTEM, report: report(null, null) })).toEqual({
      showHint: false,
      markSeen: false,
    })
  })

  it('leaves the flag unspent when the system default is the only editor', () => {
    // Someone who installs Sublime Text next month is still owed the hint.
    expect(decideEditorHint({ hintSeen: false, storedChoice: SYSTEM, report: report(false) })).toEqual({
      showHint: false,
      markSeen: false,
    })
  })

  it('shows the hint and spends the flag when another editor is installed', () => {
    expect(decideEditorHint({ hintSeen: false, storedChoice: SYSTEM, report: report(true) })).toEqual({
      showHint: true,
      markSeen: true,
    })
  })
})
