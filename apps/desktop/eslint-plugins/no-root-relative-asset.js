/**
 * ESLint rule: a static asset's URL goes through SvelteKit's `asset()`, never a root-relative
 * literal.
 *
 * Rationale: the desktop app serves at the root, so `/icons/x.svg` happens to work there. The
 * browser demo (getcmdr.com/demo, `src/lib/demo/CLAUDE.md`) serves the same frontend under the
 * `/demo` base, where that literal 404s: the Servers view's empty-state image broke exactly this
 * way. `asset('icons/x.svg')` from `$app/paths` prefixes the base (`''` in the app, so desktop
 * output is unchanged).
 *
 *   import { asset } from '$app/paths'
 *   <img src={asset('icons/network-no-hosts.svg')} alt="" />
 *
 * ## What this rule catches
 *
 * A path literal whose first segment is a top-level entry of `static/` (`/icons/…`, `/logo.svg`,
 * `/favicon.png`), in:
 * - a string literal or a template literal's leading text, in script or TS code;
 * - a static Svelte attribute value (`src="/icons/x.svg"`);
 * - a `url(/…)` in a Svelte `<style>` block.
 *
 * The `static/` listing is read once at load, so a new top-level folder there is covered with no
 * edit here. Keying on `static/`'s entries (not on a file extension) keeps real filesystem paths
 * like `/Applications/Foo.app/icon.png` and routes like `/settings` out of it.
 *
 * ## What it deliberately does NOT catch
 *
 * - Plain `.css` files (ESLint doesn't lint them); there are none referencing `static/` today.
 * - A computed path (`'/' + dir + '/x.svg'`): the leading `'/'` alone names no asset.
 * - Tests: they assert the root-base OUTPUT (`/icons/sync-synced.svg`) and mock icon URLs, so the
 *   config turns the rule off for `*.test.ts`.
 *
 * Option (for tests): `{ staticEntries: ['icons', 'logo.svg'] }` replaces the `static/` listing.
 */

import { readdirSync } from 'node:fs'
import { URL, fileURLToPath } from 'node:url'

function readStaticEntries() {
  try {
    return readdirSync(fileURLToPath(new URL('../static', import.meta.url)))
  } catch {
    return []
  }
}

const defaultStaticEntries = readStaticEntries()

/** The `static/` entry a root-relative path starts with, or undefined. */
function staticEntryOf(value, entries) {
  if (typeof value !== 'string' || value[0] !== '/' || value[1] === '/') return undefined
  const first = value.slice(1).split(/[/?#]/, 1)[0]
  return entries.has(first) ? first : undefined
}

const CSS_URL = /url\(\s*(['"]?)(\/[^)'"\s]*)\1\s*\)/g

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description: "Reference a `static/` asset through `$app/paths`' `asset()`, never a root-relative literal.",
      recommended: true,
    },
    messages: {
      rootRelativeAsset:
        "`{{ path }}` is root-relative, so it 404s under a SvelteKit base (the browser demo serves at `/demo`). Use `asset('{{ fixed }}')` from `$app/paths`: it adds the base, which is `''` in the app.",
    },
    schema: [
      {
        type: 'object',
        properties: { staticEntries: { type: 'array', items: { type: 'string' } } },
        additionalProperties: false,
      },
    ],
  },
  create(context) {
    const entries = new Set(context.options[0]?.staticEntries ?? defaultStaticEntries)
    if (entries.size === 0) return {}

    function check(node, value) {
      if (!staticEntryOf(value, entries)) return
      context.report({ node, messageId: 'rootRelativeAsset', data: { path: value, fixed: value.slice(1) } })
    }

    return {
      Literal(node) {
        check(node, node.value)
      },
      TemplateLiteral(node) {
        check(node, node.quasis[0]?.value.cooked)
      },
      SvelteLiteral(node) {
        check(node, node.value)
      },
      SvelteStyleElement(node) {
        const text = context.sourceCode.getText(node)
        for (const match of text.matchAll(CSS_URL)) {
          const path = match[2]
          if (!staticEntryOf(path, entries)) continue
          const start = node.range[0] + match.index
          context.report({
            loc: {
              start: context.sourceCode.getLocFromIndex(start),
              end: context.sourceCode.getLocFromIndex(start + match[0].length),
            },
            messageId: 'rootRelativeAsset',
            data: { path, fixed: path.slice(1) },
          })
        }
      },
    }
  },
}
