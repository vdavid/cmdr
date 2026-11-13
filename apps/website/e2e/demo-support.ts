import { test, type Page } from '@playwright/test'
import { existsSync } from 'node:fs'

// The browser demo at /demo/ (issue #302): the real desktop frontend on a fake backend, copied into
// dist/ only by a `CMDR_WEBSITE_DEMO=1` build (src/plugins/demo.ts). Two suites test it, as two
// Playwright projects (`playwright.config.ts`):
// - `demo-smoke.spec.ts`, in the `chromium` project with every other spec: does the demo boot and
//   list a folder. The website lane runs it against the RELEASED demo image, which can predate
//   `main`'s frontend, so it asks only what every release's demo answers.
// - `demo.spec.ts`, the `demo` and `demo-mac` projects (with the smoke; a Windows visitor and a Mac
//   one, since the demo answers each with their own OS's keys and copy): the whole demo, against a demo built from
//   the same sources (the release pipeline's `demo` job, before it publishes, and the nightly CI
//   run). `apps/website/DETAILS.md` § Browser demo.
// CI always builds the site with a demo, so there a missing one fails the run; a plain local
// `pnpm build` skips both suites.
export function skipWithoutDemo(): void {
  const demoBuilt = existsSync(new URL('../dist/demo/index.html', import.meta.url))
  test.skip(
    !demoBuilt && !process.env.CI,
    'dist/demo/ isn’t built: run `pnpm build:demo` at the repo root, then `CMDR_WEBSITE_DEMO=1 pnpm build` here',
  )
}

/**
 * What the demo shows this visitor, by the OS their user agent names (the demo's own rule,
 * `apps/desktop/src/lib/demo/platform.ts`): the keys under their hands and their OS's words.
 */
export interface Visitor {
  /** The main shortcut modifier as Playwright spells it: ⌘ on a Mac, Ctrl elsewhere. */
  mod: 'Meta' | 'Control'
  /** Opens the operation log (`log.operationLog`). */
  operationLogKey: string
  /** The delete dialog's warning on a volume with nowhere to put deleted files. */
  noTrashWarning: string
}

export async function visitorOf(page: Page): Promise<Visitor> {
  const userAgent = await page.evaluate(() => navigator.userAgent)
  if (/Macintosh|Mac OS X/.test(userAgent)) {
    return { mod: 'Meta', operationLogKey: 'Meta+Alt+l', noTrashWarning: 'This volume doesn’t support trash.' }
  }
  const isWindows = userAgent.includes('Windows')
  return {
    mod: 'Control',
    operationLogKey: 'Control+Shift+L',
    noTrashWarning: isWindows ? 'This volume doesn’t have a Recycle Bin.' : 'This volume doesn’t support trash.',
  }
}

/** Collects every uncaught exception and unhandled rejection the page throws. */
export function collectPageErrors(page: Page): Error[] {
  const errors: Error[] = []
  page.on('pageerror', (error) => errors.push(error))
  return errors
}
