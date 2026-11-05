import { RuleTester } from 'eslint'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import tseslint from 'typescript-eslint'
import rule from './webkit-floor-dom.js'

// The rule needs type information, so the code is linted as `file.ts` inside a fixture project
// whose `lib` is the app's (ES2022 + DOM).
const fixtureDir = join(dirname(fileURLToPath(import.meta.url)), 'fixtures', 'webkit-floor-dom')
const filename = join(fixtureDir, 'file.ts')

const ruleTester = new RuleTester({
  languageOptions: {
    parser: tseslint.parser,
    parserOptions: {
      project: join(fixtureDir, 'tsconfig.json'),
      tsconfigRootDir: fixtureDir,
      // Under `CI=true`, typescript-eslint infers a single run and builds the program once from the
      // files on DISK, so every case would be type-checked as the placeholder `file.ts` and the
      // rule would see no DOM calls at all (all five invalid cases passed locally, failed in CI).
      disallowAutomaticSingleRunInference: true,
    },
  },
  plugins: { cmdr: { rules: { 'webkit-floor-dom': rule } } },
})

const tooNew = (api, since, callee) => ({ messageId: 'tooNew', data: { api, since, callee } })

ruleTester.run('cmdr/webkit-floor-dom', rule, {
  valid: [
    // What Safari 15.4 has: a DOM method, a global function, a constructor.
    { code: `document.body.append('x'); structuredClone({}); new ResizeObserver(() => {})`, filename },
    // A library's own method with a DOM-sounding name isn't the DOM's.
    { code: `const logger = { checkVisibility: () => true }; logger.checkVisibility()`, filename },
    // Feature checks, in each shape the app writes them.
    { code: `const el = document.body; if (typeof el.checkVisibility === 'function') el.checkVisibility()`, filename },
    { code: `const el = document.body; 'checkVisibility' in el && el.checkVisibility()`, filename },
    { code: `document.body.checkVisibility?.()`, filename },
    {
      code: `typeof requestIdleCallback === 'function' ? requestIdleCallback(() => {}) : setTimeout(() => {}, 0)`,
      filename,
    },
    { code: `if (typeof OffscreenCanvas !== 'undefined') new OffscreenCanvas(1, 1)`, filename },
  ],
  invalid: [
    {
      code: `document.body.checkVisibility()`,
      filename,
      errors: [tooNew('Element.checkVisibility', 'added in Safari 17.4', 'document.body.checkVisibility')],
    },
    {
      code: `AbortSignal.timeout(1000)`,
      filename,
      errors: [tooNew('AbortSignal.timeout', 'added in Safari 16', 'AbortSignal.timeout')],
    },
    {
      code: `requestIdleCallback(() => {})`,
      filename,
      errors: [tooNew('requestIdleCallback', 'not in any Safari yet', 'requestIdleCallback')],
    },
    {
      code: `new OffscreenCanvas(1, 1)`,
      filename,
      errors: [tooNew('OffscreenCanvas', 'added in Safari 16.4', 'OffscreenCanvas')],
    },
    // A check on a different name doesn't guard this one.
    {
      code: `const el = document.body; if (typeof el.animate === 'function') el.checkVisibility()`,
      filename,
      errors: [tooNew('Element.checkVisibility', 'added in Safari 17.4', 'el.checkVisibility')],
    },
  ],
})
