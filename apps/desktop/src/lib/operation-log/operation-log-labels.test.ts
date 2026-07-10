/**
 * Unit tests for the pure operation-log label mapping. Every enum variant maps to
 * a resolved English catalog string, and the summary formats from kind + count via
 * ICU plural (thousands separator on the count). Exhaustive by construction — a new
 * enum variant would fail to compile in the source switch, and any missing case
 * here would surface as a wrong string.
 */

import { describe, it, expect } from 'vitest'
import type { ArchiveSubkind, ExecutionStatus, Initiator, ItemOutcome, OpKind, RollbackState } from '$lib/ipc/bindings'
import {
  operationSummary,
  initiatorLabel,
  executionStatusLabel,
  rollbackStateLabel,
  itemOutcomeLabel,
} from './operation-log-labels'

describe('operationSummary', () => {
  it('formats each op kind from the typed kind + count', () => {
    expect(operationSummary('copy', null, 3)).toBe('Copied 3 items')
    expect(operationSummary('move', null, 3)).toBe('Moved 3 items')
    expect(operationSummary('delete', null, 3)).toBe('Deleted 3 items')
    expect(operationSummary('trash', null, 3)).toBe('Moved 3 items to the trash')
    expect(operationSummary('rename', null, 3)).toBe('Renamed 3 items')
    expect(operationSummary('createFolder', null, 1)).toBe('Created 1 folder')
    expect(operationSummary('createFile', null, 1)).toBe('Created 1 file')
  })

  it('resolves the archive_edit subkind (compress vs edit vs extract)', () => {
    expect(operationSummary('archiveEdit', 'compress', 5)).toBe('Compressed 5 items')
    expect(operationSummary('archiveEdit', 'edit', 1)).toBe('Edited an archive')
    expect(operationSummary('archiveEdit', 'extract', 1)).toBe('Extracted an archive')
    // A missing/unknown subkind falls back to the generic archive-edit label.
    expect(operationSummary('archiveEdit', null, 1)).toBe('Edited an archive')
  })

  it('uses the singular plural branch and a thousands separator', () => {
    expect(operationSummary('copy', null, 1)).toBe('Copied 1 item')
    expect(operationSummary('delete', null, 1_234)).toBe('Deleted 1,234 items')
  })

  it('covers every OpKind (no unmapped kind)', () => {
    const kinds: OpKind[] = ['copy', 'move', 'delete', 'trash', 'rename', 'createFolder', 'createFile', 'archiveEdit']
    for (const kind of kinds) expect(operationSummary(kind, null, 2)).toBeTruthy()
    const subkinds: ArchiveSubkind[] = ['compress', 'edit', 'extract']
    for (const sub of subkinds) expect(operationSummary('archiveEdit', sub, 2)).toBeTruthy()
  })
})

describe('enum labels', () => {
  it('maps every initiator', () => {
    const cases: Record<Initiator, string> = {
      user: 'You',
      aiClient: 'AI client',
      agent: 'Agent',
    }
    for (const [value, label] of Object.entries(cases)) {
      expect(initiatorLabel(value as Initiator)).toBe(label)
    }
  })

  it('maps every execution status, avoiding "failed"', () => {
    const cases: Record<ExecutionStatus, string> = {
      queued: 'Queued',
      running: 'Running',
      done: 'Done',
      failed: 'Didn’t finish',
      canceled: 'Canceled',
    }
    for (const [value, label] of Object.entries(cases)) {
      expect(executionStatusLabel(value as ExecutionStatus)).toBe(label)
    }
  })

  it('maps every rollback state', () => {
    const cases: Record<RollbackState, string> = {
      notRollbackable: 'Can’t roll back',
      rollbackable: 'Can roll back',
      rollingBack: 'Rolling back',
      rolledBack: 'Rolled back',
      partiallyRolledBack: 'Partly rolled back',
    }
    for (const [value, label] of Object.entries(cases)) {
      expect(rollbackStateLabel(value as RollbackState)).toBe(label)
    }
  })

  it('maps every item outcome', () => {
    const cases: Record<ItemOutcome, string> = {
      done: 'Done',
      skipped: 'Skipped',
      failed: 'Didn’t finish',
      rolledBack: 'Rolled back',
    }
    for (const [value, label] of Object.entries(cases)) {
      expect(itemOutcomeLabel(value as ItemOutcome)).toBe(label)
    }
  })
})
