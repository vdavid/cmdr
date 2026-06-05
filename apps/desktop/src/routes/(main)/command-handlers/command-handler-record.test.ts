/**
 * The durable completeness guard for the dispatch handler record.
 *
 * The `CommandHandlerRecord` type (keyed by `Exclude<CommandId, DispatchExemptId>`)
 * already forces completeness at compile time: a missing handler fails to
 * compile, an exempt-id handler fails to compile. These runtime checks back that
 * with a set-equality assertion (so a failure names the offending id instead of a
 * wall of TS errors) and a `@ts-expect-error` proving a bogus exempt id (one not
 * in `CommandId`) is rejected. Together they make "add a command → make a
 * decision" enforced: a new id needs either a handler or an entry in
 * `DISPATCH_EXEMPT_IDS`.
 *
 * The command modules pull in `$lib/tauri-commands` and the quick-look /
 * settings / updater singletons at import time, so mock the leaf side effects the
 * same way the dispatch characterization suite does. The record's KEYS are what
 * we assert on, not the handler bodies, so the mocks just need to make the
 * imports side-effect-free.
 */
import { describe, it, expect, vi } from 'vitest'

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ info: vi.fn(), debug: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }))
vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn() }))
vi.mock('$lib/settings', () => ({ getSetting: vi.fn(() => 100), setSetting: vi.fn() }))
vi.mock('$lib/shortcuts', () => ({ getEffectiveShortcuts: vi.fn(() => []) }))
vi.mock('$lib/settings/settings-window', () => ({ openSettingsWindow: vi.fn() }))
vi.mock('$lib/error-reporter/error-report-flow.svelte', () => ({ openErrorReportDialog: vi.fn() }))
vi.mock('$lib/updates/updater.svelte', () => ({ runMenuTriggeredCheck: vi.fn() }))
vi.mock('$lib/downloads/go-to-latest', () => ({ goToLatestDownload: vi.fn() }))
vi.mock('$lib/tauri-commands', () => ({
  openExternalUrl: vi.fn(),
  showInFinder: vi.fn(),
  copyToClipboard: vi.fn(),
  quickLookOpen: vi.fn(),
  quickLookClose: vi.fn(),
  getInfo: vi.fn(),
  openInEditor: vi.fn(),
  syncMenuShowHidden: vi.fn(),
  readClipboardText: vi.fn(() => Promise.resolve('')),
  cloudMakeAvailableOffline: vi.fn(),
  cloudRemoveDownload: vi.fn(),
}))
vi.mock('$lib/file-explorer/pane/focused-pane-reads', () => ({
  getFocusedPaneVolumeId: vi.fn(() => 'local'),
  getFocusedPanePath: vi.fn(() => '/Users/test'),
}))
vi.mock('$lib/file-explorer/quick-look/quick-look-state.svelte', () => ({
  quickLookState: { isOpen: false },
  quickLookDispatchGuardJustFired: vi.fn(() => false),
  armQuickLookDispatchGuard: vi.fn(),
}))

import { commandHandlers } from './index'
import { DISPATCH_EXEMPT_IDS, type DispatchExemptId } from './types'
import { COMMAND_IDS, type CommandId } from '$lib/commands'

describe('command handler record completeness', () => {
  it('handler keys ∪ exempt ids = COMMAND_IDS, disjoint', () => {
    const handlerKeys = Object.keys(commandHandlers)
    const exemptIds: readonly string[] = DISPATCH_EXEMPT_IDS

    // Disjoint: no id is both handled and exempt.
    const overlap = handlerKeys.filter((id) => exemptIds.includes(id))
    expect(overlap, 'ids both handled and exempt').toEqual([])

    // Union equals the full id set (nothing missing, nothing extra).
    const union = new Set([...handlerKeys, ...exemptIds])
    expect(union).toEqual(new Set(COMMAND_IDS))
    expect(handlerKeys.length + exemptIds.length).toBe(COMMAND_IDS.length)
  })

  it('exposes a handler for every dispatchable id (no missing key)', () => {
    const exemptSet = new Set<string>(DISPATCH_EXEMPT_IDS)
    for (const id of COMMAND_IDS) {
      if (exemptSet.has(id)) continue
      expect(commandHandlers[id as Exclude<CommandId, DispatchExemptId>], `missing handler for ${id}`).toBeTypeOf(
        'function',
      )
    }
  })

  it('the exempt tuple is exactly 20 ids, all real CommandIds', () => {
    expect(DISPATCH_EXEMPT_IDS).toHaveLength(20)
    for (const id of DISPATCH_EXEMPT_IDS) expect(COMMAND_IDS).toContain(id)
  })
})

describe('DispatchExemptId is a subset of CommandId (compile-time)', () => {
  // Enforced by `tsc` / `svelte-check`, not at runtime; the runtime `expect`
  // only lets Vitest execute the block.
  it('rejects a bogus exempt id that is not a CommandId', () => {
    const real: DispatchExemptId = 'app.quit'

    // @ts-expect-error -- 'app.doesNotExist' is not a member of CommandId, so it
    // can't be a DispatchExemptId (the union is carved out of CommandId).
    const bogus: DispatchExemptId = 'app.doesNotExist'

    expect(real).toBe('app.quit')
    expect(bogus).toBe('app.doesNotExist')
  })
})
