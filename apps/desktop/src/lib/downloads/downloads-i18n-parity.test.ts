/**
 * Base-locale (en) parity net for the downloads i18n migration.
 *
 * The download-toast copy, the global-shortcut row/warn-toast copy, the
 * latest-download empty/FDA toasts, the macOS notification strings, and the
 * toggle description moved from hardcoded English into the `downloads.*` catalog.
 * This is a behavior-preserving MOVE: every rendered en string must be
 * byte-identical to the pre-migration copy. These goldens are the literals that
 * lived in the downloads components and helpers before the move; a future copy
 * edit lands in the catalog AND here together, never silently.
 *
 * `t()` (not `tString`) is used for the rich-text (`<Trans>`) keys so the array
 * output can be flattened and asserted as plain text.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { t, tString } from '$lib/intl/messages.svelte'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

/** Flatten a rich-text `t()` result (array of strings + tag-handler returns) to text. */
function flatten(result: ReturnType<typeof t>): string {
  if (typeof result === 'string') return result
  return (result as unknown[]).map((p) => (typeof p === 'string' ? p : '')).join('')
}

/** A tag handler returning its inner chunks joined as text (mirrors a passthrough wrapper). */
const tag = (chunks: unknown[]): string => (chunks as string[]).join('')

describe('downloads catalog parity (en)', () => {
  it('resolves the download toast title and subdir line', () => {
    expect(flatten(t('downloads.toast.downloaded', { fileName: 'report.pdf', file: tag }))).toBe(
      'Downloaded report.pdf',
    )
    expect(tString('downloads.toast.inSubdir', { subdir: 'Downloads/Chrome/' })).toBe('in Downloads/Chrome/')
  })

  it('resolves the toast teaching copy and hints', () => {
    expect(tString('downloads.toast.learnIntro')).toBe('Something cool to learn about jumping to downloads')
    expect(flatten(t('downloads.toast.inAppHint', { key: '⌘J', chip: tag }))).toBe('In-app: Press ⌘J to jump here')
    expect(flatten(t('downloads.toast.globalHint', { key: '⌃⌥⌘J', chip: tag, em: tag }))).toBe(
      'In any app (global shortcut), press ⌃⌥⌘J',
    )
  })

  it('resolves the collapsed summary variants', () => {
    expect(
      flatten(
        t('downloads.toast.summaryBoth', {
          inAppKey: '⌘J',
          globalKey: '⌃⌥⌘J',
          inApp: tag,
          global: tag,
        }),
      ),
    ).toBe('Jump with ⌘J in-app, ⌃⌥⌘J globally.')
    expect(flatten(t('downloads.toast.summaryInApp', { inAppKey: '⌘J', inApp: tag }))).toBe('Jump with ⌘J in-app.')
    expect(flatten(t('downloads.toast.summaryGlobal', { globalKey: '⌃⌥⌘J', global: tag }))).toBe(
      'Jump with ⌃⌥⌘J globally.',
    )
  })

  it('resolves the toast affordances and buttons', () => {
    expect(tString('downloads.toast.expandTip')).toBe('Show the shortcut tip')
    expect(tString('downloads.toast.collapseTip')).toBe('Make this notification more compact')
    expect(tString('downloads.toast.stopShowing')).toBe('Stop showing these')
    expect(tString('downloads.toast.jumpToFile')).toBe('Jump to file')
  })

  it('resolves the macOS notification title', () => {
    expect(tString('downloads.notification.title', { fileName: 'report.pdf' })).toBe('Downloaded report.pdf')
  })

  it('resolves the empty-downloads toast', () => {
    expect(tString('downloads.empty.message')).toBe('Your Downloads folder is empty. Go there anyway?')
    expect(tString('downloads.empty.dismiss')).toBe('Dismiss')
    expect(tString('downloads.empty.goToDownloads')).toBe('Go to Downloads')
  })

  it('resolves the Full-Disk-Access toast', () => {
    expect(tString('downloads.fda.message')).toBe('Cmdr needs Full Disk Access to watch your Downloads folder.')
    expect(tString('downloads.fda.dismiss')).toBe('Dismiss')
    expect(tString('downloads.fda.openSystemSettings')).toBe('Open System Settings')
  })

  it('resolves the global-shortcut warn toast', () => {
    expect(tString('downloads.warnToast.message', { binding: '⌃⌥⌘J' })).toBe(
      'The ⌃⌥⌘J shortcut jumps to your latest download from anywhere. Keep it on?',
    )
    expect(tString('downloads.warnToast.turnOff')).toBe('Turn it off')
    expect(tString('downloads.warnToast.keepOn')).toBe('Keep it on')
  })

  it('resolves the global-shortcut row copy (including the apostrophe in the warning statuses)', () => {
    expect(tString('downloads.shortcutRow.scopeTitle')).toBe('Global')
    expect(flatten(t('downloads.shortcutRow.commandName', { marker: tag }))).toBe('Go to latest download (global)')
    expect(tString('downloads.shortcutRow.modifiedTooltip')).toBe('Modified from default')
    expect(tString('downloads.shortcutRow.resetTooltip')).toBe('Reset to default')
    expect(tString('downloads.shortcutRow.pressKeys')).toBe('Press keys...')
    expect(tString('downloads.shortcutRow.registered')).toBe('Registered')
    expect(tString('downloads.shortcutRow.notRegistered')).toBe('Not registered')
    expect(tString('downloads.shortcutRow.invalidCombo')).toBe("Couldn't register: invalid combo")
    expect(tString('downloads.shortcutRow.registerFailed', { reason: 'busy' })).toBe("Couldn't register: busy")
    expect(tString('downloads.shortcutRow.addModifier')).toBe('Add a modifier (⌘, ⌃, ⌥, or ⇧)')
  })

  it('resolves the toggle description (bound and unbound)', () => {
    expect(tString('downloads.toggleDescription.bound', { binding: '⌃⌥⌘J' })).toBe(
      'Press ⌃⌥⌘J from any app to jump to your most recent download.',
    )
    expect(tString('downloads.toggleDescription.unbound')).toBe('Jump to your most recent download from any app.')
  })
})
