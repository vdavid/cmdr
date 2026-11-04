/**
 * The demo's menu bar state: `DemoMenuBar` from the WASM engine (`crates/cmdr-listing-wasm/src/menu.rs`
 * over `crates/cmdr-menu`). It answers the menu-state pushes the app sends the native menu
 * (`activate_window_menu`, `update_menu_accelerator`, `set_commands_refused_over_dialog`, …) and
 * draws the macOS bar from them, so this file holds no menu logic: it loads, routes, and types the
 * answers. See `DETAILS.md` § The menu bar.
 */
import type { Payload } from './answer-types.js'
import { loadEngineModule } from './engine-module.js'

/** A row's words: a `menu.*` catalog key with its `{token}` values, or a literal (`Cmdr`). */
export type MenuRowLabel = { key: string; args: Record<string, string> } | { text: string }

/** One row of a drawn menu, as `menu.rs`'s `Row` serializes. */
export type MenuRow =
  | { kind: 'item'; id: string; label: MenuRowLabel; shortcut: string | null; enabled: boolean }
  | { kind: 'check'; id: string; label: MenuRowLabel; shortcut: string | null; enabled: boolean; checked: boolean }
  | { kind: 'submenu'; label: MenuRowLabel; enabled: boolean; rows: MenuRow[] }
  | { kind: 'system'; label: MenuRowLabel }
  | { kind: 'separator' }

/** One top-level menu of the drawn bar. */
export interface BarMenu {
  id: string
  title: MenuRowLabel
  rows: MenuRow[]
}

/** What a click asks the page to do, as `menu.rs`'s `Click` serializes. */
export type MenuClick =
  | { kind: 'emit'; event: string; payload: unknown }
  | { kind: 'nativeEdit'; action: 'cut' | 'copy' | 'paste' | 'selectAll' }
  | { kind: 'closeFocusedWindow' }
  | { kind: 'nothing' }

export interface MenuEngineHandle {
  /** Whether `invoke` answers `cmd`. Waits for the engine, so an early push isn't lost. */
  handles(cmd: string): Promise<boolean>
  /** Stores one push, as `invoke` would answer it. */
  invoke(cmd: string, payload: Payload): Promise<unknown>
  /** The bar as it stands, or `null` before (or without) the engine. */
  bar(): BarMenu[] | null
  /** What a click on `menuId` sends. */
  click(menuId: string): MenuClick
  /** Resolves once the engine answered, with whether it did. */
  readonly ready: Promise<boolean>
}

export function startMenuEngine(): MenuEngineHandle {
  let menu: InstanceType<typeof import('virtual:cmdr-listing-wasm').DemoMenuBar> | null = null
  let DemoMenuBar: typeof import('virtual:cmdr-listing-wasm').DemoMenuBar | null = null
  const ready = loadEngineModule().then((wasm) => {
    if (!wasm) return false
    DemoMenuBar = wasm.DemoMenuBar
    menu = new wasm.DemoMenuBar()
    return true
  })

  return {
    ready,
    handles: async (cmd) => (await ready) && DemoMenuBar !== null && DemoMenuBar.handles(cmd),
    invoke: async (cmd, payload) => {
      await ready
      if (!menu) throw new Error(`[demo] the menu bar did not load, so it can't answer ${cmd}`)
      return menu.invoke(cmd, payload)
    },
    bar: () => (menu ? (menu.bar() as BarMenu[]) : null),
    click: (menuId) => (menu ? (menu.click(menuId) as MenuClick) : { kind: 'nothing' }),
  }
}
