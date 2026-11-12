/**
 * Linux: a dialog gets the `Alt+<letter>` keys the menu bar's mnemonics would take.
 *
 * GTK takes a bar title's `Alt+<letter>` (English: F, E, S, V, G, T, H) before WebKit sees the
 * keydown, so `preventDefault` can't help. While a dialog, an overlay, or the palette is up, the main
 * window's mnemonic modifier is a mask nobody presses (`src-tauri/src/menu/mnemonic_mask.rs`), and
 * Alt again once the last one closes.
 *
 * Real X key events only (`helpers/x11-keys.ts`): `pressKey`'s synthetic DOM event never reaches
 * GTK, so it would pass with or without the mask. An open GTK menu is a new visible top-level X
 * window, which is how the spec sees it, and a key GTK took never shows in the page's own keydown
 * record. One language proves it: the mask doesn't read labels. The search dialog's `Alt+F` and
 * every Multi-rename option key (`Alt+<letter>` off macOS, read from the registry's keymap) are
 * covered, the second from a text field and from an option control.
 */
import os from 'node:os'
import { test, expect } from './fixtures.js'
import {
  dispatchMenuCommand,
  ensureAppReady,
  escapeOverlayUntilGone,
  moveCursorToFile,
  pressKey,
  type PageLike,
} from './helpers.js'
import { ensureMcpClient } from '../e2e-shared/mcp-client.js'
import { focusMainWindow, pressRealKey, visibleTopLevelWindows } from './helpers/x11-keys.js'
import { defaultKey } from './helpers/registry-keys.js'
import { closeSearchDialog, getActiveMode, openSearchDialog, pollActiveMode, SEARCH_INPUT } from './search-helpers.js'
import { waitBudget } from './wait-budget.js'

/** The X windows that appeared since `before`: an open menu is one. */
function newWindowsSince(before: readonly string[]): string[] {
  return visibleTopLevelWindows().filter((id) => !before.includes(id))
}

test.describe('Menu bar mnemonics while a dialog is up (Linux)', () => {
  test.skip(os.platform() !== 'linux', 'Only the Linux bar has mnemonics')

  test('a real Alt+F reaches the search dialog, and opens the File menu once it closes', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const atStart = visibleTopLevelWindows()
    try {
      await openSearchDialog(tauriPage)
      await tauriPage.evaluate(`(function(){
          var el = document.querySelector(${JSON.stringify(SEARCH_INPUT)});
          if (el) el.focus();
      })()`)
      await tauriPage.waitForFunction(
        `document.activeElement === document.querySelector(${JSON.stringify(SEARCH_INPUT)})`,
        waitBudget(3000),
      )
      // Start from Regex, so Filename mode can only come from the real Alt+F.
      await pressKey(tauriPage, 'Alt+r', 'KeyR')
      expect(await pollActiveMode(tauriPage, 'regex')).toBe(true)

      focusMainWindow()
      pressRealKey('alt+f')
      const switched = await pollActiveMode(tauriPage, 'filename', waitBudget(3000))
      expect({ switched, menusOpened: newWindowsSince(atStart) }).toEqual({ switched: true, menusOpened: [] })

      await closeSearchDialog(tauriPage)
      expect(await getActiveMode(tauriPage)).toBeNull()

      focusMainWindow()
      pressRealKey('alt+f')
      await expect.poll(() => newWindowsSince(atStart).length, { timeout: waitBudget(3000) }).toBeGreaterThan(0)

      pressRealKey('Escape')
      await expect.poll(() => newWindowsSince(atStart), { timeout: waitBudget(3000) }).toEqual([])
    } finally {
      // A failed run can leave the File menu open (it grabs the keyboard) or the dialog up.
      if (newWindowsSince(atStart).length > 0) pressRealKey('Escape')
      if ((await getActiveMode(tauriPage)) !== null) await closeSearchDialog(tauriPage)
    }
  })

  test('every Multi-rename Alt+<letter> key reaches the sheet, from a text field and from an option', async ({
    tauriPage,
  }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    expect(await moveCursorToFile(tauriPage, 'file-a.txt')).toBe(true)
    const atStart = visibleTopLevelWindows()
    try {
      for (const target of [SHEET_SEARCH_FIELD, SHEET_FIRST_OPTION]) {
        await dispatchMenuCommand(tauriPage, 'file.multiRename')
        await tauriPage.waitForSelector(`${SHEET} .sheet`, waitBudget(5000))
        await installKeyRecorder(tauriPage)

        // The toggles first, each pressed twice so the sheet ends as it began (it reopens on its
        // last settings); the two that open something, then Undo rename (nothing to undo), last.
        for (const commandId of [...SHEET_TOGGLES, ...SHEET_OPENERS]) {
          const chord = toXdotoolChord(defaultKey(commandId))
          const presses = SHEET_TOGGLES.includes(commandId) ? 2 : 1
          for (let press = 0; press < presses; press++) {
            const before = await optionStates(tauriPage)
            await focusIn(tauriPage, target)
            await tauriPage.evaluate('window.__keydownsSeen = []')
            focusMainWindow()
            pressRealKey(chord)
            await expect
              .poll(() => keydownsSeen(tauriPage), { timeout: waitBudget(3000) })
              .toContain(defaultKey(commandId))
            const after = {
              menusOpened: newWindowsSince(atStart),
              fieldText: await searchFieldText(tauriPage),
              toggled: (await optionStates(tauriPage)) !== before,
            }
            expect({ commandId, target, ...after }).toEqual({
              commandId,
              target,
              menusOpened: [],
              fieldText: '',
              toggled: SHEET_TOGGLES.includes(commandId),
            })
          }
        }

        // Go to parent's `Alt+↑` and Go back's `Alt+←` are bar accelerators, greyed while the sheet
        // is up, and the sheet's rows and fields use them: a greyed item's accelerator must not eat them.
        for (const [chord, seen] of [
          ['alt+Up', 'Alt+ArrowUp'],
          ['alt+Left', 'Alt+ArrowLeft'],
        ] as const) {
          await focusIn(tauriPage, target)
          await tauriPage.evaluate('window.__keydownsSeen = []')
          focusMainWindow()
          pressRealKey(chord)
          await expect.poll(() => keydownsSeen(tauriPage), { timeout: waitBudget(3000) }).toContain(seen)
          expect(newWindowsSince(atStart)).toEqual([])
        }
        await escapeOverlayUntilGone(tauriPage, SHEET)
      }
    } finally {
      // A failed run can leave a menu open (it grabs the keyboard) or the sheet up.
      if (newWindowsSince(atStart).length > 0) pressRealKey('Escape')
      if ((await tauriPage.count(SHEET)) > 0) await escapeOverlayUntilGone(tauriPage, SHEET)
      await removeKeyRecorder(tauriPage)
    }
  })
})

const SHEET = '[data-dialog-id="multi-rename"]'
/** The Search field: the first plain field in the grid (the two masks sit in `.masks`). */
const SHEET_SEARCH_FIELD = `${SHEET} .field-grid > label.field input`
/** The first whole-name checkbox's input, which takes focus as the control. */
const SHEET_FIRST_OPTION = `${SHEET} .options .option-control input`

/** The eight option keys that flip a switch. */
const SHEET_TOGGLES: readonly string[] = [
  'multiRename.removeDiacritics',
  'multiRename.greekToLatin',
  'multiRename.normalizeUnicode',
  'multiRename.matchCase',
  'multiRename.firstMatchOnly',
  'multiRename.includeExtension',
  'multiRename.regex',
  'multiRename.replaceWholeName',
]
/** The other three: Letter case's menu, the media field picker, and Undo rename. */
const SHEET_OPENERS: readonly string[] = ['multiRename.letterCase', 'multiRename.insertField', 'multiRename.undoRename']

/** A Playwright chord (`Alt+r`) in xdotool's spelling (`alt+r`). */
function toXdotoolChord(chord: string): string {
  const XDOTOOL: Record<string, string> = { Control: 'ctrl', Alt: 'alt', Shift: 'shift', Meta: 'super' }
  return chord
    .split('+')
    .map((part) => XDOTOOL[part] ?? part)
    .join('+')
}

/** Records every keydown the page sees, as `defaultKey` spells it, so a key GTK took shows as missing. */
async function installKeyRecorder(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`(function(){
      window.__keydownsSeen = [];
      if (window.__keyRecorder) return;
      window.__keyRecorder = function(e){
        var mods = [];
        if (e.ctrlKey) mods.push('Control');
        if (e.altKey) mods.push('Alt');
        if (e.shiftKey) mods.push('Shift');
        if (e.metaKey) mods.push('Meta');
        var key = e.code.indexOf('Key') === 0 ? e.code.slice(3).toLowerCase() : e.key;
        window.__keydownsSeen.push(mods.concat([key]).join('+'));
      };
      window.addEventListener('keydown', window.__keyRecorder, true);
  })()`)
}

async function removeKeyRecorder(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`(function(){
      if (window.__keyRecorder) window.removeEventListener('keydown', window.__keyRecorder, true);
      window.__keyRecorder = undefined;
      window.__keydownsSeen = undefined;
  })()`)
}

function keydownsSeen(tauriPage: PageLike): Promise<string[]> {
  return tauriPage.evaluate<string[]>('window.__keydownsSeen || []')
}

/** Every option's on/off state in the sheet, as one string. */
function optionStates(tauriPage: PageLike): Promise<string> {
  return tauriPage.evaluate<string>(`Array.prototype.map.call(
      document.querySelectorAll('${SHEET} [aria-pressed], ${SHEET} .checkbox-control[data-state]'),
      function(el){ return el.getAttribute('aria-pressed') || el.getAttribute('data-state'); }
  ).join(',')`)
}

function searchFieldText(tauriPage: PageLike): Promise<string> {
  return tauriPage.evaluate<string>(`document.querySelector(${JSON.stringify(SHEET_SEARCH_FIELD)})?.value ?? 'missing'`)
}

/**
 * Puts focus on `selector` and keeps asking until it stays there: the sheet places its own focus
 * once it has settled after opening (`open-settled.ts`), which can land after a first `focus()`.
 */
async function focusIn(tauriPage: PageLike, selector: string): Promise<void> {
  const sel = JSON.stringify(selector)
  await expect
    .poll(
      () =>
        tauriPage.evaluate<boolean>(`(function(){
            var el = document.querySelector(${sel});
            if (!el) return false;
            if (document.activeElement !== el) el.focus();
            return document.activeElement === el;
        })()`),
      { timeout: waitBudget(5000) },
    )
    .toBe(true)
}
