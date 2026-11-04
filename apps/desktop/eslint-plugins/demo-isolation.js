/**
 * ESLint rule: the browser demo stays out of app code.
 *
 * The demo (`src/lib/demo/`, getcmdr.com/demo) is the real app on a fake backend. Its whole
 * app-side footprint is one import in `src/hooks.client.ts` plus the `__CMDR_DEMO_BUILD__` global
 * declared in `src/app.d.ts` (and defined by the build config). Demo behavior goes through the
 * app's existing seams (IPC answers, store seeds, typed refusals), never an `if (demo)` branch:
 * a branch would make the demo stop testing the app it shows, and an import of a demo module
 * from app code would ship demo code to every user (only `#lib/demo/boot.js` is aliased away
 * outside the demo build). See `src/lib/demo/CLAUDE.md`.
 *
 * Flags, outside `src/lib/demo/`:
 * - any `__CMDR_DEMO_BUILD__` reference (except its declaration in `src/app.d.ts`);
 * - any import, re-export, or dynamic `import()` of `#lib/demo/…` or a relative `lib/demo/` path
 *   (except `src/hooks.client.ts`'s import of the boot module).
 *
 * No opt-out: a need for one means the demo needs a seam, not a branch.
 */

import { posix } from 'node:path'

const demoDir = '/src/lib/demo/'
const flagAllowed = ['/src/app.d.ts']
const importAllowed = ['/src/hooks.client.ts']

/** True for a module specifier, imported from `fromFile`, that reaches into the demo. */
function isDemoSource(value, fromFile) {
  if (value === '#lib/demo' || value.startsWith('#lib/demo/')) return true
  if (!value.startsWith('.')) return false
  return `${posix.join(posix.dirname(fromFile), value)}/`.includes(demoDir)
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description: 'Keep the browser demo out of app code: no demo flag, no demo imports.',
      recommended: true,
    },
    messages: {
      demoFlag:
        "Don't branch on `__CMDR_DEMO_BUILD__` in app code. Give the demo what it needs through an existing seam " +
        '(an IPC answer, a store seed, a typed refusal) in `src/lib/demo/`. See `src/lib/demo/CLAUDE.md`.',
      demoImport:
        "Don't import `{{ source }}` from app code: only `#lib/demo/boot.js` is kept out of non-demo builds, so " +
        'this would ship demo code to every user. See `src/lib/demo/CLAUDE.md`.',
    },
    schema: [],
  },
  create(context) {
    const filename = (context.filename || context.getFilename() || '').replaceAll('\\', '/')
    const normalized = filename.startsWith('/') ? filename : `/${filename}`
    if (normalized.includes(demoDir)) return {}
    const flagOk = flagAllowed.some((fragment) => normalized.endsWith(fragment))
    const importOk = importAllowed.some((fragment) => normalized.endsWith(fragment))

    function checkSource(source) {
      if (importOk || !source || source.type !== 'Literal' || typeof source.value !== 'string') return
      if (!isDemoSource(source.value, normalized)) return
      context.report({ node: source, messageId: 'demoImport', data: { source: source.value } })
    }

    return {
      Identifier(node) {
        if (!flagOk && node.name === '__CMDR_DEMO_BUILD__') context.report({ node, messageId: 'demoFlag' })
      },
      ImportDeclaration: (node) => checkSource(node.source),
      ExportNamedDeclaration: (node) => checkSource(node.source),
      ExportAllDeclaration: (node) => checkSource(node.source),
      ImportExpression: (node) => checkSource(node.source),
    }
  },
}
