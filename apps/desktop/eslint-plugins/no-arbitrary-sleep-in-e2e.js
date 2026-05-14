/**
 * ESLint rule: ban `await sleep(N)` in E2E spec files.
 *
 * Rationale: every fixed-duration sleep in an E2E test is a margin that's
 * either too tight (flake) or too loose (slow). The Step 2 speedup pass found
 * that 80% of Playwright wall-clock was fixed sleeps; replacing them with
 * `pollUntil` / `waitForSelector` cut wall-clock in half. New code should not
 * recreate the same pattern.
 *
 * What this rule flags:
 *   await sleep(<any-arg>)              // most common (`sleep` imported from helpers.ts)
 *   await helpers.sleep(<any-arg>)      // qualified call form
 *
 * Scope: only `*.spec.ts` files inside `test/e2e-playwright/`. Helper files
 * (helpers.ts, conflict-helpers.ts, etc.) are NOT linted: they implement
 * `pollUntil` itself, which legitimately calls `sleep(interval)` between
 * iterations.
 *
 * Opt out per-line for genuine fixed-duration waits (e.g., file-watcher
 * debounce settling, where no observable signal exists to poll against):
 *
 *   // eslint-disable-next-line cmdr/no-arbitrary-sleep-in-e2e -- <reason>
 *   await sleep(500)
 *
 * Prefer `pollUntil(...)` / `tauriPage.waitForSelector(...)` /
 * `tauriPage.waitForFunction(...)` over an opt-out.
 *
 * See docs/testing.md § "❌ `await sleep(N)` in E2E specs" for the full
 * rationale and replacement patterns.
 */

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Use `pollUntil` or `waitForSelector` in E2E specs; fixed sleeps are flaky-or-slow. See docs/testing.md.',
      recommended: true,
    },
    messages: {
      sleepInE2E:
        '`await sleep({{ arg }})` in an E2E spec is a fixed margin, either too tight (flake) or too loose (slow). ' +
        'Replace with `pollUntil(page, async () => …, timeout)` or `page.waitForSelector(selector, timeout)`. ' +
        'See `docs/testing.md` § "❌ `await sleep(N)` in E2E specs". ' +
        'Opt out per-line with `// eslint-disable-next-line cmdr/no-arbitrary-sleep-in-e2e -- <reason>` only when a ' +
        'genuine fixed wait is needed.',
    },
    schema: [],
  },
  create(context) {
    return {
      CallExpression(node) {
        const callee = node.callee

        // Match: sleep(...)   OR   <anything>.sleep(...)
        const isBareSleep = callee.type === 'Identifier' && callee.name === 'sleep'
        const isMemberSleep =
          callee.type === 'MemberExpression' &&
          !callee.computed &&
          callee.property.type === 'Identifier' &&
          callee.property.name === 'sleep'

        if (!isBareSleep && !isMemberSleep) return

        // Render the arg as a short string for the error message
        const firstArg = node.arguments[0]
        let argText = '...'
        if (firstArg) {
          if (firstArg.type === 'Literal') {
            argText = String(firstArg.value)
          } else {
            const source = context.sourceCode ?? context.getSourceCode?.()
            if (source) {
              argText = source.getText(firstArg)
              if (argText.length > 40) argText = argText.slice(0, 40) + '…'
            }
          }
        }

        context.report({
          node,
          messageId: 'sleepInE2E',
          data: { arg: argText },
        })
      },
    }
  },
}
