/**
 * Finds CSS in an emitted stylesheet that Safari 15.4 (the floor; a fully patched Catalina gets
 * 15.6.1) drops, read from MDN's browser-compat-data through `vite-webkit-floor-compat.ts`.
 *
 * ## Why
 *
 * `build.cssTarget` lowers what it can (`oklch()` to `lab()`, nesting, prefixes), but a feature
 * with no lowering ships as written, and WebKit drops what it can't parse in silence: a property
 * goes unset, a whole rule goes if one selector in its list is unknown, a whole block goes with
 * an unknown at-rule. That's how container queries once dropped layout on old Safari and
 * `color-mix()` dropped accent colors (stylelint bans the first, `@supports not` fallbacks in
 * `app.css` cover the second). Nothing crashes, so no smoke test notices.
 *
 * ## What it checks, and what passes
 *
 * Properties (and their keyword values, `text-wrap: balance`), functions in values,
 * pseudo-classes and pseudo-elements, at-rules, and media features. A feature passes when the
 * cascade already handles it:
 *
 * - it sits inside an `@supports` block (whoever wrote that knew);
 * - an unprefixed property has its supported `-webkit-` twin in the same block (the build adds
 *   these for the target);
 * - a value has an earlier declaration of the same property in the block (the fallback);
 * - an `@supports not (…)` block anywhere in the stylesheets names it (the app's `color-mix()`
 *   fallbacks are shaped like this).
 *
 * Custom property values (`--x: …`) aren't parsed: they're inert until a `var()` uses them,
 * and the property that does is what the check reads.
 */

import { generate, parse, walk, type CssNode } from 'css-tree'
import { cssData, cssFunctions, describeSince, isBeyond, SAFARI_FLOOR } from './vite-webkit-floor-compat.ts'

type Feature = Parameters<typeof isBeyond>[0]

/**
 * Features a person checked and accepted, keyed by the finding's own label. `harmless`: an
 * enhancement whose absence looks fine. `degrades`: a known Catalina degradation, tracked in the
 * issue its reason names, so it stays visible rather than quietly accepted. Anything new fails the
 * build until it gets a fallback or an entry here.
 */
export const REVIEWED_CSS: Readonly<Record<string, { impact: 'harmless' | 'degrades'; why: string }>> = {
  'scrollbar-gutter': {
    impact: 'harmless',
    why: 'macOS overlay scrollbars take no space, so there is no gutter to keep',
  },
  'scrollbar-width': { impact: 'harmless', why: 'only thins a scrollbar; old WebKit draws the default one' },
  'overscroll-behavior': { impact: 'harmless', why: 'scroll chaining and rubber-banding come back, nothing else' },
  'text-wrap': { impact: 'harmless', why: 'balanced or pretty line breaks fall back to ordinary ones' },
  'interpolate-size': { impact: 'harmless', why: 'an auto-height transition snaps instead of animating' },
  'overflow-anchor': { impact: 'harmless', why: 'scroll anchoring stays at the browser default' },
  'text-decoration: dotted': {
    impact: 'harmless',
    why: "`abbr[title]`'s dotted underline in the reset; Cmdr writes no `<abbr title>`",
  },
  '-webkit-text-decoration: dotted': { impact: 'harmless', why: 'the prefixed twin of the same reset line' },
  'clip: rect()': {
    impact: 'harmless',
    why: 'the legacy `clip: rect(…)` of the visually-hidden utility, which every WebKit reads',
  },
  'align-content: start': {
    impact: 'degrades',
    why: 'Safari 15.6 has it (a patched Catalina); 15.4–15.5 lose it. #994',
  },
  'grid-template-columns: subgrid': {
    impact: 'degrades',
    why: 'servers hub and acknowledgements rows stack their cells. #994',
  },
  '::highlight': { impact: 'degrades', why: "Ask Cmdr's ⌘F finds matches but paints none. #994" },
  'contain: inline-size': { impact: 'degrades', why: 'a long name can widen the conflict dialogs. #994' },
  'overflow: clip': { impact: 'degrades', why: 'the Settings shell falls back to `visible`. #994' },
}

export interface CssFinding {
  /** `backdrop-filter`, `text-wrap: balance`, `color-mix()`, `:user-invalid`, `@container`, `(prefers-reduced-transparency)`. */
  feature: string
  since: string
}

const FUNCTIONS = cssFunctions()
const child = (parent: Feature, key: string) => (parent as Record<string, Feature> | undefined)?.[key]

/** `-webkit-backdrop-filter` → `{ prefix: '-webkit-', name: 'backdrop-filter' }`. */
function splitPrefix(name: string): { prefix: string | undefined; name: string } {
  const match = /^(-(?:webkit|moz|ms|o)-)(.+)$/.exec(name)
  return match ? { prefix: match[1], name: match[2] } : { prefix: undefined, name }
}

/** `@supports`'s own grammar, which a `name(` match would otherwise read as a tested function. */
const SUPPORTS_KEYWORDS = new Set(['not', 'and', 'or', 'selector', 'font-format', 'font-tech'])

/** The properties and functions every `@supports not (…)` prelude tests. */
export function supportsNotFallbacks(css: string): Set<string> {
  const names = new Set<string>()
  walk(parse(css, { parseValue: false }), {
    visit: 'Atrule',
    enter(node) {
      if (node.name !== 'supports' || !node.prelude) return
      const prelude = generate(node.prelude)
      if (!/^\s*not\b/.test(prelude)) return
      // The properties (`name:`) and functions (`name(`) it tests, not their arguments.
      for (const word of prelude.matchAll(/([a-z-]+)\s*[:(]/g)) {
        if (!SUPPORTS_KEYWORDS.has(word[1])) names.add(word[1])
      }
    },
  })
  return names
}

/** Every feature in `css` that Safari 15.4 drops and nothing in the cascade covers. */
export function findCssBeyondSafari15(css: string, fallbacks: ReadonlySet<string> = new Set()): CssFinding[] {
  const findings: CssFinding[] = []
  const seen = new Set<string>()
  const report = (feature: string, entry: Feature, prefix?: string) => {
    if (seen.has(feature) || feature in REVIEWED_CSS) return
    seen.add(feature)
    findings.push({ feature, since: describeSince(entry, prefix) })
  }
  const beyond = (entry: Feature, prefix?: string) => isBeyond(entry, SAFARI_FLOOR, prefix)

  const ast = parse(css, { parseCustomProperty: false })
  walk(ast, {
    enter(node: CssNode) {
      if (node.type === 'Atrule') {
        if (node.name === 'supports') return walk.skip
        const entry = child(cssData['at-rules'], splitPrefix(node.name).name)
        if (beyond(entry, splitPrefix(node.name).prefix)) report(`@${node.name}`, entry)
        return undefined
      }
      if (node.type === 'Feature' && node.kind === 'media') {
        const entry = child(child(cssData['at-rules'], 'media'), node.name)
        if (beyond(entry) && !fallbacks.has(node.name)) report(`(${node.name})`, entry)
        return undefined
      }
      if (node.type === 'PseudoClassSelector' || node.type === 'PseudoElementSelector') {
        const { prefix, name } = splitPrefix(node.name)
        const entry = child(cssData.selectors, name)
        const sigil = node.type === 'PseudoClassSelector' ? ':' : '::'
        if (beyond(entry, prefix) && !fallbacks.has(name)) report(`${sigil}${node.name}`, entry, prefix)
        return undefined
      }
      if (node.type === 'Block') checkBlock(node, { beyond, report, fallbacks })
      return undefined
    },
  })
  return findings
}

interface BlockContext {
  beyond: (entry: Feature, prefix?: string) => boolean
  report: (feature: string, entry: Feature, prefix?: string) => void
  fallbacks: ReadonlySet<string>
}

/** One declaration block: its properties, their keyword values, and the functions in them. */
function checkBlock(block: CssNode & { type: 'Block' }, ctx: BlockContext): void {
  const declarations = block.children.toArray().filter((n) => n.type === 'Declaration')
  const present = new Set(declarations.map((d) => d.property))
  const earlier = new Set<string>()
  for (const declaration of declarations) {
    const { prefix, name } = splitPrefix(declaration.property)
    if (name.startsWith('-')) continue // A custom property: inert until a `var()` reads it.
    const property = child(cssData.properties, name)
    const twinCovers = prefix === undefined && present.has(`-webkit-${name}`) && !ctx.beyond(property, '-webkit-')
    const propertyIsNew = ctx.beyond(property, prefix) && !twinCovers
    if (propertyIsNew && !ctx.fallbacks.has(name)) ctx.report(declaration.property, property, prefix)
    // A new property's keywords go with it; a fallback declaration earlier in the block covers a value.
    if (!propertyIsNew && !earlier.has(declaration.property)) checkValue(declaration, property, ctx)
    earlier.add(declaration.property)
  }
}

/** A declaration's keyword values and functions. */
function checkValue(declaration: CssNode & { type: 'Declaration' }, property: Feature, ctx: BlockContext): void {
  walk(declaration.value, {
    enter(node: CssNode) {
      if (node.type === 'Identifier') {
        const keyword = child(property, node.name)
        if (ctx.beyond(keyword) && !ctx.fallbacks.has(node.name)) {
          ctx.report(`${declaration.property}: ${node.name}`, keyword)
        }
      }
      if (node.type === 'Function') {
        // Named with its property: `rect()` is a new shape in `clip-path` but the ancient syntax of `clip`.
        const fn = FUNCTIONS.get(node.name)
        if (ctx.beyond(fn) && !ctx.fallbacks.has(node.name)) ctx.report(`${declaration.property}: ${node.name}()`, fn)
      }
    },
  })
}

/** The build error's lines for the CSS findings, grouped by stylesheet. */
export function describeCssFindings(byFile: ReadonlyMap<string, readonly CssFinding[]>): string[] {
  const lines = [
    'The stylesheets use CSS that Safari 15.4 drops without a word (a fully patched macOS Catalina gets Safari 15.6.1),',
    'and the build can’t lower it. Give each one a fallback: an earlier declaration of the same property, a',
    '`@supports` block, or an `@supports not (…)` block naming it. Why: `scripts/vite-webkit-css-floor.ts`.',
  ]
  for (const [file, findings] of byFile) {
    for (const f of findings) lines.push(`  ${file}: ${f.feature} (${f.since})`)
  }
  return lines
}
