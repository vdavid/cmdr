/**
 * Functional tests for `KeyboardShortcutsSection.svelte`'s edit flow.
 *
 * Focus: the "+ add" flow and the conflict banner must never leak a junk `''`
 * entry into the store (the old shape materialized a real `addShortcut(id, '')`
 * the instant the user clicked +, and clicking away leaked a framed `(none)`
 * pill). "Adding" is now pure UI state: a synthetic editing pill renders when
 * `editingShortcut` targets one-past-the-end, and the store is only touched when
 * a key is actually captured and confirmed.
 *
 * The Tauri boundaries (plugin-store, the cross-window event bus, the IPC
 * bindings, the settings store) are mocked so the real `$lib/shortcuts` store
 * runs end-to-end against an in-memory disk, exactly like `shortcuts-store.test`.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'

// In-memory disk for the fake plugin-store (mirrors shortcuts-store.test.ts).
const disk = vi.hoisted(() => new Map<string, unknown>())

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn((_path: string, opts?: { defaults?: Record<string, unknown> }) => {
    for (const [k, v] of Object.entries(opts?.defaults ?? {})) {
      if (!disk.has(k)) disk.set(k, v)
    }
    return Promise.resolve({
      get: (key: string) => Promise.resolve(disk.get(key)),
      set: (key: string, value: unknown) => {
        disk.set(key, value)
        return Promise.resolve()
      },
      delete: (key: string) => Promise.resolve(disk.delete(key)),
      keys: () => Promise.resolve([...disk.keys()]),
      save: () => Promise.resolve(),
    })
  }),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/settings/store-path', () => ({
  resolveStorePath: (name: string) => Promise.resolve(name),
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    updateMenuAccelerator: () => Promise.resolve({ status: 'ok' as const, data: null }),
    // GlobalShortcutRow reads the global binding on mount.
    setGlobalGoToLatestShortcut: () => Promise.resolve({ status: 'ok' as const, data: null }),
  },
}))

// GlobalShortcutRow (rendered at the bottom of the section) reads settings.
vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => undefined),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/utils/confirm-dialog', () => ({
  confirmDialog: vi.fn(() => Promise.resolve(false)),
}))

import KeyboardShortcutsSection from './KeyboardShortcutsSection.svelte'
import { initializeShortcuts, getEffectiveShortcuts, isShortcutModified, resetAllShortcuts } from '$lib/shortcuts'

// `app.about` (About Cmdr): scope App, default [] — David's original repro row.
const ABOUT = 'app.about'
// `file.copy` lives under File list and defaults to ['F5'].
const COPY = 'file.copy'

let target: HTMLElement
let component: ReturnType<typeof mount> | null = null

async function flushSave(): Promise<void> {
  for (let i = 0; i < 5; i++) await Promise.resolve()
}

/** Find the `.command-row` whose anchor id encodes `commandId`. */
function row(commandId: string): HTMLElement {
  const el = target.querySelector<HTMLElement>(`[id$="${commandId}"]`)
  if (!el) throw new Error(`row for ${commandId} not found`)
  return el
}

/** All shortcut pills (real + synthetic editing) inside a command row. */
function pills(commandId: string): HTMLButtonElement[] {
  return [...row(commandId).querySelectorAll<HTMLButtonElement>('.shortcut-pill')]
}

function addButton(commandId: string): HTMLButtonElement {
  const el = row(commandId).querySelector<HTMLButtonElement>('.add-shortcut')
  if (!el) throw new Error(`add button for ${commandId} not found`)
  return el
}

function clickAddShortcut(commandId: string): void {
  addButton(commandId).click()
  flushSync()
}

/** Dispatch a keydown on the document (the section listens in capture phase). */
function pressKey(init: KeyboardEventInit): void {
  document.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, ...init }))
  flushSync()
}

beforeEach(async () => {
  disk.clear()
  // `$lib/shortcuts` is a static import, so its module-scoped `customShortcuts`
  // map is shared across tests in this file. `initializeShortcuts` is a one-shot
  // (guarded by an `initialized` flag), so reset the map explicitly between tests
  // rather than re-importing — clear any customization a prior test persisted.
  await initializeShortcuts()
  await resetAllShortcuts()
  target = document.createElement('div')
  document.body.appendChild(target)
})

afterEach(() => {
  if (component) {
    void unmount(component)
    component = null
  }
  target.remove()
})

function render(): void {
  component = mount(KeyboardShortcutsSection, { target, props: { searchQuery: '' } })
  flushSync()
}

describe('KeyboardShortcutsSection add flow', () => {
  it('clicking + then clicking away leaves no entry in the store and no framed (none) pill', async () => {
    render()
    expect(isShortcutModified(ABOUT)).toBe(false)

    clickAddShortcut(ABOUT)
    // A synthetic editing pill appears (the add slot), but nothing is in the store yet.
    await flushSave()
    expect(getEffectiveShortcuts(ABOUT)).toEqual([])
    expect(isShortcutModified(ABOUT)).toBe(false)

    // Click away: focus another row's + (a different command). The add slot must vanish.
    clickAddShortcut(COPY)
    await flushSave()

    expect(isShortcutModified(ABOUT)).toBe(false)
    expect(getEffectiveShortcuts(ABOUT)).toEqual([])
    // No leaked pill on the About row (it has no shortcuts and is not being edited).
    expect(pills(ABOUT)).toHaveLength(0)
    expect(disk.has(`shortcut:${ABOUT}`)).toBe(false)
  })

  it('starting an add on two rows in sequence leaves no leak on either', async () => {
    render()
    clickAddShortcut(ABOUT)
    clickAddShortcut(COPY) // moving to a second row's add
    await flushSave()
    // Cancel the second too by pressing Escape.
    pressKey({ key: 'Escape' })
    await flushSave()

    expect(isShortcutModified(ABOUT)).toBe(false)
    expect(isShortcutModified(COPY)).toBe(false)
    expect(pills(ABOUT)).toHaveLength(0)
    expect(disk.has(`shortcut:${ABOUT}`)).toBe(false)
    expect(disk.has(`shortcut:${COPY}`)).toBe(false)
  })

  it('Escape mid-add leaves no leak', async () => {
    render()
    clickAddShortcut(ABOUT)
    pressKey({ key: 'Escape' })
    await flushSave()

    expect(isShortcutModified(ABOUT)).toBe(false)
    expect(pills(ABOUT)).toHaveLength(0)
    expect(disk.has(`shortcut:${ABOUT}`)).toBe(false)
  })

  it('capturing a key on the add slot then confirming creates exactly one entry', async () => {
    render()
    clickAddShortcut(ABOUT)
    pressKey({ key: 'F12' }) // F12 is unbound, so no conflict; the save path runs
    // The section confirms 500ms after the last keypress; wait it out for real.
    await new Promise((r) => setTimeout(r, 600))
    flushSync()
    await flushSave()

    const shortcuts = getEffectiveShortcuts(ABOUT)
    expect(shortcuts).toEqual(['F12'])
    expect(isShortcutModified(ABOUT)).toBe(true)
  })
})

describe('KeyboardShortcutsSection conflict banner', () => {
  it('shows the banner with the proposed combo and keeps the pill in a pending-decision state', async () => {
    render()
    // Bind F5 (file.copy default) onto the add slot of About — conflict.
    clickAddShortcut(ABOUT)
    pressKey({ key: 'F5' })
    await flushSave()

    const banner = target.querySelector('.conflict-warning')
    expect(banner).not.toBeNull()
    // Nothing persisted while the decision is pending.
    expect(isShortcutModified(ABOUT)).toBe(false)
    // The editing pill renders the proposed combo, flagged as pending-decision.
    const editing = row(ABOUT).querySelector('.shortcut-pill.editing')
    expect(editing).not.toBeNull()
    expect(editing?.classList.contains('pending-conflict')).toBe(true)
  })

  it('Cancel in the banner exits edit mode cleanly with nothing persisted', async () => {
    render()
    clickAddShortcut(ABOUT)
    pressKey({ key: 'F5' })
    await flushSave()
    expect(target.querySelector('.conflict-warning')).not.toBeNull()

    const cancelBtn = [...target.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Cancel')
    expect(cancelBtn).toBeTruthy()
    cancelBtn?.click()
    flushSync()
    await flushSave()

    expect(target.querySelector('.conflict-warning')).toBeNull()
    expect(row(ABOUT).querySelector('.shortcut-pill.editing')).toBeNull()
    expect(isShortcutModified(ABOUT)).toBe(false)
  })

  it('clicking a different pill while the banner is up dismisses the banner', async () => {
    render()
    clickAddShortcut(ABOUT)
    pressKey({ key: 'F5' })
    await flushSave()
    expect(target.querySelector('.conflict-warning')).not.toBeNull()

    // Click file.copy's existing F5 pill to start editing it.
    const copyPill = pills(COPY)[0]
    copyPill.click()
    flushSync()

    expect(target.querySelector('.conflict-warning')).toBeNull()
  })

  it('clicking + on another row while the banner is up dismisses the banner', async () => {
    render()
    clickAddShortcut(ABOUT)
    pressKey({ key: 'F5' })
    await flushSave()
    expect(target.querySelector('.conflict-warning')).not.toBeNull()

    clickAddShortcut(COPY)
    expect(target.querySelector('.conflict-warning')).toBeNull()
  })
})
