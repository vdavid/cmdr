/**
 * E2E for what Ask Cmdr can do from the rail, and what it can't finish alone, against the real
 * webview. A message `[tool:copy] {"pane":"left"}` makes the rail's fake call that tool through the
 * real gate (`chat/e2e_fake.rs`, `rail_fake_tool_trigger`), so every call here is the agent's.
 *
 * ⚠️ An E2E run is direct mode for outside clients, so `mcpCall` can do what the agent can't. The
 * agent never gets direct mode, which is the point of the fence tests: `dialog_confirm` works over
 * MCP in this very run. `agent/chat/runtime/capability_tests.rs` walks the whole matrix (every origin,
 * every tool); this spec proves the part only a webview can.
 *
 * A dialog a scripted turn opens is guarded, so a person-confirm test waits for `data-armed`, ❌ never
 * a sleep.
 */

import fs from 'fs'
import path from 'path'
import { waitBudget } from './wait-budget.js'
import { test, expect } from './fixtures.js'
import { closeRailIfOpen, ensureChatReady, LAST_BUBBLE_JS, openRailViaMenu, typeAndSend } from './ask-cmdr-helpers.js'
import { confirmRollback } from './conflict-helpers.js'
import {
  clickButtonByText,
  dismissAllToasts,
  dispatchMenuCommand,
  drainOperations,
  ensureAppReady,
  getFixtureRoot,
  waitForTransferUiToSettle,
} from './helpers.js'
import { restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { ensureMcpClient, mcpCall, mcpNavToPath, mcpReadResource } from '../e2e-shared/mcp-client.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const TRANSFER = 'transfer-confirmation'
const DELETE = 'delete-confirmation'
const ROLLBACK = 'rollback-confirmation'
const RENAME_CONFLICT = 'rename-conflict'
const SKIP_CONFIRMATION = 'fileOperations.skipConfirmation'

function dialogSelector(dialog: string): string {
  return `[data-dialog-id="${dialog}"]`
}

function dialogIsOpen(page: TauriPage, dialog: string): Promise<boolean> {
  return page.evaluate<boolean>(`document.querySelector('${dialogSelector(dialog)}') !== null`)
}

/** What `data-armed` says on the dialog's overlay, or `null` when it isn't there or isn't guarded. */
function armedStateOf(page: TauriPage, dialog: string): Promise<string | null> {
  return page.evaluate<string | null>(`(() => {
    const el = document.querySelector('${dialogSelector(dialog)}');
    const holder = el?.closest('[data-armed]') ?? el?.querySelector('[data-armed]');
    return holder?.getAttribute('data-armed') ?? null;
  })()`)
}

async function untilOpenAndArmed(page: TauriPage, dialog: string): Promise<void> {
  await expect.poll(() => armedStateOf(page, dialog), { timeout: waitBudget(5000) }).toBe('true')
}

function lastBubbleText(page: TauriPage): Promise<string> {
  return page.evaluate<string>(`(${LAST_BUBBLE_JS})?.textContent || ''`)
}

/** Sends `[tool:<name>] <args>` on a fresh thread, and waits for the turn's sign-off. */
async function agentCalls(page: TauriPage, tool: string, args: unknown = {}): Promise<void> {
  await page.evaluate(`document.querySelector('.ask-cmdr-rail [aria-label="New chat"]')?.click()`)
  await expect
    .poll(() => page.evaluate<number>(`document.querySelectorAll('.ask-cmdr-rail .msg').length`), {
      timeout: waitBudget(3000),
    })
    .toBe(0)
  await typeAndSend(page, `[tool:${tool}] ${JSON.stringify(args)}`)
  await expect
    .poll(() => lastBubbleText(page), { timeout: waitBudget(15000) })
    .toContain("That's the tool, from the test assistant.")
}

/** The same call, through the wrapper a deferred tool goes through. */
function agentCallsThroughUseTool(page: TauriPage, tool: string, params: unknown = {}): Promise<void> {
  return agentCalls(page, 'use_tool', { name: tool, params })
}

function state(): Promise<string> {
  return mcpReadResource('cmdr://state')
}

/** Both panes on the fixture tree, with `file-a.txt` selected on the left. */
async function selectFileA(fixtureRoot: string): Promise<void> {
  await mcpNavToPath('left', path.join(fixtureRoot, 'left'))
  await mcpNavToPath('right', path.join(fixtureRoot, 'right'))
  await mcpCall('select', { pane: 'left', names: ['file-a.txt'] })
}

async function closeDialog(page: TauriPage, dialog: string): Promise<void> {
  await mcpCall('dialog', { action: 'close', type: dialog })
  await expect.poll(() => dialogIsOpen(page, dialog), { timeout: waitBudget(3000) }).toBe(false)
}

interface OperationRow {
  opId: string
  initiator: string
  rollbackState: string
  inverseOpId: string | null
  rollsBackOpId: string | null
}

async function operation(operationId: string): Promise<OperationRow> {
  const detail = JSON.parse(await mcpCall('operations_get', { operationId, limit: 1 })) as { operation: OperationRow }
  return detail.operation
}

/** The newest logged operation. */
async function latestOperation(): Promise<OperationRow> {
  const list = JSON.parse(await mcpCall('operations_list', { limit: 1 })) as { operations: OperationRow[] }
  return list.operations[0]
}

test.describe('Ask Cmdr from the rail', () => {
  test.beforeEach(async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    recreateFixtures(getFixtureRoot())
    await ensureAppReady(page)
    await ensureMcpClient(page)
    await openRailViaMenu(page)
    await ensureChatReady(page)
  })

  // ONE hook, drain THEN restore: `DETAILS.md` § "The fixture-tree leak guard".
  test.afterEach(async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    await closeRailIfOpen(page)
    await drainOperations(page)
    await dismissAllToasts(page)
    restoreFixtureTree(getFixtureRoot())
  })

  test('navigates, selects, opens a tab, and sorts, the deferred ones through use_tool', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const right = path.join(getFixtureRoot(), 'right')
    const left = path.join(getFixtureRoot(), 'left')

    await agentCalls(page, 'nav_to_path', { pane: 'right', path: right })
    await expect.poll(state, { timeout: waitBudget(5000) }).toContain(`path: ${right}`)

    await agentCalls(page, 'nav_to_path', { pane: 'left', path: left })
    await agentCalls(page, 'select', { pane: 'left', names: ['file-a.txt', 'file-b.txt'] })
    await expect.poll(state, { timeout: waitBudget(5000) }).toContain('selected: 2')

    await agentCallsThroughUseTool(page, 'sort', { pane: 'left', by: 'size', order: 'desc' })
    await expect.poll(state, { timeout: waitBudget(5000) }).toContain('sort: "size:desc"')
    await mcpCall('sort', { pane: 'left', by: 'name', order: 'asc' })

    const tabs = async (): Promise<number> =>
      (await state()).split('\n').filter((line) => /^\s+- i:\d+ id:/.test(line)).length
    const before = await tabs()
    await agentCallsThroughUseTool(page, 'tab', { pane: 'left', action: 'new' })
    await expect.poll(tabs, { timeout: waitBudget(5000) }).toBe(before + 1)
    await mcpCall('tab', { pane: 'left', action: 'close' })
    await expect.poll(tabs, { timeout: waitBudget(5000) }).toBe(before)
  })

  test('reads the logs through use_tool, and the rail names what it read', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    await agentCallsThroughUseTool(page, 'read_resource', { uri: 'cmdr://logs?limit=1' })
    expect(await lastBubbleText(page)).not.toContain('Used a tool')
  })

  test('creates a folder by name, with no dialog', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    await selectFileA(fixtureRoot)
    await agentCalls(page, 'mkdir', { pane: 'right', name: 'made-by-the-agent' })
    await expect
      .poll(() => fs.existsSync(path.join(fixtureRoot, 'right', 'made-by-the-agent')), { timeout: waitBudget(5000) })
      .toBeTruthy()
    expect(await dialogIsOpen(page, 'mkdir-confirmation')).toBe(false)
  })

  test('opens copy, move, delete, and rename for a person, and nothing moves on disk', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    const source = path.join(fixtureRoot, 'left', 'file-a.txt')
    await selectFileA(fixtureRoot)

    for (const [tool, dialog] of [
      ['copy', TRANSFER],
      ['move', TRANSFER],
      ['delete', DELETE],
    ]) {
      await agentCalls(page, tool, { pane: 'left' })
      await untilOpenAndArmed(page, dialog)
      expect(
        await page.evaluate<string>(`document.querySelector('${dialogSelector(dialog)}')?.textContent || ''`),
      ).toContain('Ask Cmdr started this.')
      expect(await lastBubbleText(page)).not.toContain('OK:')
      await closeDialog(page, dialog)
      expect(fs.existsSync(source)).toBe(true)
      expect(fs.existsSync(path.join(fixtureRoot, 'right', 'file-a.txt'))).toBe(false)
    }

    await agentCalls(page, 'rename', { pane: 'left', name: 'file-a.txt', newName: 'renamed.txt' })
    await expect
      .poll(
        () =>
          page.evaluate<string | null>(`document.querySelector('.rename-input')?.getAttribute('data-armed') ?? null`),
        {
          timeout: waitBudget(5000),
        },
      )
      .toBe('true')
    await mcpCall('dialog', { action: 'close', type: 'rename' })
    await expect
      .poll(() => page.evaluate<boolean>(`document.querySelector('.rename-input') === null`), {
        timeout: waitBudget(3000),
      })
      .toBe(true)
    expect(fs.existsSync(source)).toBe(true)
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'renamed.txt'))).toBe(false)
  })

  // The agent picks the name, so what that name runs into is guarded too: the Enter that commits the
  // editor mustn't answer the question it raises.
  test('a rename onto a name already taken asks through a dialog that has to arm', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    await selectFileA(fixtureRoot)

    await agentCalls(page, 'rename', { pane: 'left', name: 'file-a.txt', newName: 'file-b.txt' })
    await expect
      .poll(
        () =>
          page.evaluate<string | null>(`document.querySelector('.rename-input')?.getAttribute('data-armed') ?? null`),
        { timeout: waitBudget(5000) },
      )
      .toBe('true')
    await page.press('.rename-input', 'Enter')
    await untilOpenAndArmed(page, RENAME_CONFLICT)
    expect(fs.readdirSync(path.join(fixtureRoot, 'left'))).toContain('file-a.txt')

    await mcpCall('dialog', { action: 'close', type: RENAME_CONFLICT })
    await mcpCall('dialog', { action: 'close', type: 'rename' })
    await expect
      .poll(() => page.evaluate<boolean>(`document.querySelector('.rename-input') === null`), {
        timeout: waitBudget(3000),
      })
      .toBe(true)
  })

  // The fence, with a real dialog waiting to be pressed. `dialog_confirm` works in this very run for
  // an outside client, so the only thing refusing here is who's asking.
  test('can not confirm its own dialog, directly or through use_tool', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    await selectFileA(fixtureRoot)
    await agentCalls(page, 'copy', { pane: 'left' })
    await untilOpenAndArmed(page, TRANSFER)

    const confirm = { type: TRANSFER }
    await agentCalls(page, 'dialog_confirm', confirm)
    await agentCallsThroughUseTool(page, 'dialog_confirm', confirm)
    await agentCalls(page, 'use_tool', { name: 'use_tool', params: { name: 'dialog_confirm', params: confirm } })
    await agentCalls(page, 'use_tool', { name: ' dialog_confirm ', params: confirm })
    await agentCalls(page, 'dialog', { action: 'confirm', type: TRANSFER })
    await agentCalls(page, 'resolve_conflict', { operationId: 'op', conflictId: 'c', resolution: 'overwrite' })
    await agentCalls(page, 'queue_move_locked', { operationId: 'op' })

    expect(await dialogIsOpen(page, TRANSFER)).toBe(true)
    const copied = path.join(fixtureRoot, 'right', 'file-a.txt')
    expect(fs.existsSync(copied)).toBe(false)

    // Nothing was wrong with the call: the same one from an outside client, direct in this run, goes through.
    await mcpCall('dialog_confirm', confirm)
    await expect.poll(() => fs.existsSync(copied), { timeout: waitBudget(10000) }).toBeTruthy()
    await waitForTransferUiToSettle(page)
  })

  test('Skip confirmation skips what a person starts, never what the agent starts', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    await selectFileA(fixtureRoot)
    await mcpCall('set_setting', { id: SKIP_CONFIRMATION, value: true })
    try {
      for (const [tool, dialog] of [
        ['copy', TRANSFER],
        ['move', TRANSFER],
        ['delete', DELETE],
      ]) {
        await agentCalls(page, tool, { pane: 'left' })
        await untilOpenAndArmed(page, dialog)
        await agentCallsThroughUseTool(page, 'dialog_confirm', { type: dialog })
        expect(await dialogIsOpen(page, dialog)).toBe(true)
        await closeDialog(page, dialog)
      }
      expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-a.txt'))).toBe(true)

      // The setting really is on: the same copy from a person's command runs with no dialog.
      await mcpCall('select', { pane: 'left', names: ['file-a.txt'] })
      await dispatchMenuCommand(page, 'file.copy')
      await expect
        .poll(() => fs.existsSync(path.join(fixtureRoot, 'right', 'file-a.txt')), { timeout: waitBudget(10000) })
        .toBeTruthy()
      expect(await dialogIsOpen(page, TRANSFER)).toBe(false)
      await waitForTransferUiToSettle(page)
    } finally {
      await mcpCall('set_setting', { id: SKIP_CONFIRMATION, value: false })
    }
  })

  test('what a person confirms from a dialog the agent opened records the agent, rollback too', async ({
    tauriPage,
  }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    const copied = path.join(fixtureRoot, 'right', 'file-a.txt')
    await selectFileA(fixtureRoot)

    await agentCalls(page, 'copy', { pane: 'left' })
    await untilOpenAndArmed(page, TRANSFER)
    await clickButtonByText(page, `${dialogSelector(TRANSFER)} .btn-primary`, 'Copy')
    await expect.poll(() => fs.existsSync(copied), { timeout: waitBudget(10000) }).toBeTruthy()
    await waitForTransferUiToSettle(page)
    await dismissAllToasts(page)

    const copy = await latestOperation()
    expect(copy.initiator).toBe('agent')
    await expect
      .poll(async () => (await operation(copy.opId)).rollbackState, { timeout: waitBudget(15000) })
      .toBe('rollbackable')

    await agentCalls(page, 'operations_rollback', { operationId: copy.opId })
    await untilOpenAndArmed(page, ROLLBACK)
    await confirmRollback(page)
    await expect
      .poll(async () => (await operation(copy.opId)).rollbackState, { timeout: waitBudget(15000) })
      .toBe('rolledBack')
    expect(fs.existsSync(copied)).toBe(false)

    const inverseId = (await operation(copy.opId)).inverseOpId
    if (inverseId === null) throw new Error('A rolled-back operation names its inverse')
    const inverse = await operation(inverseId)
    expect(inverse.initiator).toBe('agent')
    expect(inverse.rollsBackOpId).toBe(copy.opId)
    await dismissAllToasts(page)
  })
})
