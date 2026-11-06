import { test, type Page } from '@playwright/test'
import { existsSync } from 'node:fs'

// The browser demo at /demo/ (issue #302): the real desktop frontend on a fake backend, copied into
// dist/ only by a `CMDR_WEBSITE_DEMO=1` build (src/plugins/demo.ts). Two suites test it, as two
// Playwright projects (`playwright.config.ts`):
// - `demo-smoke.spec.ts`, in the `chromium` project with every other spec: does the demo boot and
//   list a folder. The website lane runs it against the RELEASED demo image, which can predate
//   `main`'s frontend, so it asks only what every release's demo answers.
// - `demo.spec.ts`, the `demo` project (with the smoke): the whole demo, against a demo built from
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

/** Collects every uncaught exception and unhandled rejection the page throws. */
export function collectPageErrors(page: Page): Error[] {
  const errors: Error[] = []
  page.on('pageerror', (error) => errors.push(error))
  return errors
}
