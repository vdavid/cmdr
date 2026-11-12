/**
 * The item tree's words: each row's status and story, a folder's rollup, the route
 * line, and why a tree has no rows. Every one comes from a typed field through an
 * exhaustive switch, so these pin the English, the kind-awareness, and the copy rules
 * (no "error", no "failed", nothing that blames the person).
 */

import { describe, expect, it, onTestFinished } from 'vitest'
import type { OpKind, SkipReason, UnexpandableReason } from '#lib/ipc/bindings.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { withPlatform } from '#lib/platform/test-platform.js'
import {
  folderSizeText,
  operationRoute,
  skipReasonNotice,
  treeBlankNotice,
  treeRowStatus,
  treeRowTips,
  unexpandableNotice,
} from './operation-tree-labels'
import { fileRow, folderRow, tally } from './operation-tree-test-fixtures'

describe('treeRowStatus', () => {
  it('stays quiet on a row still waiting: its checkbox already says so', () => {
    expect(treeRowStatus(fileRow(1), 'copy')).toBeNull()
  })

  it('words a rolled-back row by what the reversal did to it', () => {
    const cases: [OpKind, string][] = [
      ['copy', 'Removed'],
      ['createFolder', 'Removed'],
      ['move', 'Moved back'],
      ['trash', 'Moved back'],
      ['rename', 'Renamed back'],
      ['tag', 'Tags restored'],
    ]
    for (const [kind, word] of cases) {
      expect(treeRowStatus(fileRow(1, { outcome: 'rolledBack', role: 'settled' }), kind)).toEqual({
        word,
        tone: 'rolledBack',
      })
    }
  })

  it('says skipped and didn’t finish, never failed', () => {
    expect(treeRowStatus(fileRow(1, { outcome: 'skipped', skipReason: 'drift' }), 'move')?.word).toBe('Skipped')
    expect(treeRowStatus(fileRow(1, { outcome: 'failed', role: 'settled' }), 'copy')?.word).toBe('Didn’t finish')
  })

  it('rolls a folder up from its counts, thousands separators and all', () => {
    const folder = folderRow(1, { tally: tally({ files: 20, rolledBack: 1_200, heldBack: 2 }) })
    expect(treeRowStatus(folder, 'copy')).toEqual({ word: '1,200 rolled back, 2 skipped', tone: 'rollup' })
    expect(treeRowStatus(folderRow(2, { tally: tally({ files: 3 }) }), 'copy')).toBeNull()
  })

  it('joins the two counts through the catalog, so a language picks its own separator', () => {
    // Regression: a hardcoded ASCII ", " left Chinese without its "，".
    const folder = folderRow(1, { tally: tally({ files: 20, rolledBack: 3, heldBack: 2 }) })
    expect(treeRowStatus(folder, 'copy')?.word).toBe(
      tString('operationLog.tree.rollupBoth', { rolledBack: '3 rolled back', skipped: '2 skipped' }),
    )
    expect(tString('operationLog.tree.rollupBoth', { rolledBack: 'A', skipped: 'B' })).toBe('A, B')
  })

  it('gives a folder that is itself a unit its own status first', () => {
    const unit = folderRow(1, { role: 'settled', outcome: 'rolledBack', tally: tally({ rolledBack: 1 }) })
    expect(treeRowStatus(unit, 'trash')?.word).toBe('Moved back')
  })
})

describe('treeRowTips', () => {
  it('tells a row inside a folder unit that it comes back with its folder', () => {
    expect(treeRowTips(fileRow(1, { role: 'contents' }), 'trash')).toEqual(['Comes back with its folder.'])
  })

  it('says why a rollback skipped it, or that the operation itself did', () => {
    expect(treeRowTips(fileRow(1, { outcome: 'skipped', skipReason: 'restoreTargetOccupied' }), 'move')).toEqual([
      'Something else is at the original spot now, so Cmdr didn’t overwrite it.',
    ])
    expect(treeRowTips(fileRow(1, { outcome: 'skipped', role: 'settled' }), 'copy')).toEqual([
      'The operation skipped this one, so there’s nothing to roll back.',
    ])
  })

  it('names the other end by which side the tree shows', () => {
    // A copy shows where things landed, so the other end is where they came from.
    expect(treeRowTips(fileRow(1, { otherEnd: '/src/a.txt' }), 'copy')).toEqual(['From /src/a.txt'])
    // A rename shows the old name, so the other end is where it went.
    expect(treeRowTips(fileRow(1, { otherEnd: '/src/b.txt' }), 'rename')).toEqual(['To /src/b.txt'])
  })

  it('says why a folder won’t unfold, about the operation and never this folder', () => {
    const capped = folderRow(1, { expandable: false, unexpandable: 'capped' })
    expect(treeRowTips(capped, 'trash')[0]).toContain('this operation held a folder of over 50,000 files')
  })
})

describe('the words never blame the person or reach for "error" and "failed"', () => {
  const skipReasons: SkipReason[] = [
    'unverifiablePrecondition',
    'drift',
    'restoreTargetOccupied',
    'dirNotEmpty',
    'alreadyGone',
    'vanished',
    'originalGone',
    'failed',
    'locked',
  ]
  const unexpandable: UnexpandableReason[] = ['empty', 'notRecorded', 'capped']

  it('gives every skip reason its own sentence', () => {
    const sentences = skipReasons.map((reason) => tString(skipReasonNotice(reason)))
    expect(new Set(sentences).size).toBe(skipReasons.length)
    for (const sentence of sentences) {
      for (const banned of ['error', 'failed', 'invalid']) expect(sentence.toLowerCase()).not.toContain(banned)
    }
  })

  it('gives every unexpandable reason its own sentence', () => {
    const sentences = unexpandable.map((reason) => tString(unexpandableNotice(reason)))
    expect(new Set(sentences).size).toBe(unexpandable.length)
  })
})

describe('folderSizeText', () => {
  it('counts the files under a folder, and says nothing for none', () => {
    expect(folderSizeText(tally({ files: 1 }))).toBe('1 file')
    expect(folderSizeText(tally({ files: 12_500 }))).toBe('12,500 files')
    expect(folderSizeText(tally({ files: 0 }))).toBe('')
  })
})

describe('operationRoute', () => {
  it('words the route by what the operation did', () => {
    expect(operationRoute('copy', '/Volumes/Backup', '/Users/me/Documents')).toBe(
      'From /Users/me/Documents to /Volumes/Backup',
    )
    expect(operationRoute('move', '/Volumes/Backup', null)).toBe('To /Volumes/Backup')
    expect(operationRoute('trash', '/Users/me/Downloads', null)).toBe('From /Users/me/Downloads to the Trash')
    expect(operationRoute('delete', '/tmp/x', null)).toBe('From /tmp/x')
    expect(operationRoute('rename', '/tmp/x', null)).toBe('In /tmp/x')
  })

  it('spells every route path the way Windows shows it', () => {
    onTestFinished(withPlatform('windows'))
    expect(operationRoute('copy', 'D:/Backup', 'C:/Users/test_user_1/Documents')).toBe(
      'From C:\\Users\\test_user_1\\Documents to D:\\Backup',
    )
    expect(operationRoute('move', 'D:/Backup', null)).toBe('To D:\\Backup')
    expect(operationRoute('trash', 'C:/Users/test_user_1/Downloads', null)).toMatch(
      /^From C:\\Users\\test_user_1\\Downloads /,
    )
    expect(operationRoute('delete', 'C:/tmp/x', null)).toBe('From C:\\tmp\\x')
    expect(operationRoute('rename', 'C:/tmp/x', null)).toBe('In C:\\tmp\\x')
  })

  it('names no route for a forest of volumes', () => {
    expect(operationRoute('copy', null, null)).toBeNull()
  })
})

describe('treeBlankNotice', () => {
  it('says why there are no rows, one sentence per reason', () => {
    expect(tString(treeBlankNotice({ kind: 'unbuilt', reason: 'operationRunning' }))).toContain('still running')
    expect(tString(treeBlankNotice({ kind: 'unbuilt', reason: 'rollingBack' }))).toContain('rolling back')
    expect(tString(treeBlankNotice({ kind: 'backfilling', done: 0, total: 0 }))).toContain('Reading')
    expect(tString(treeBlankNotice({ kind: 'gone' }))).toBe('This operation isn’t in your history anymore.')
    expect(tString(treeBlankNotice({ kind: 'indexesUnavailable' }))).toContain('next time you open Cmdr')
  })
})
