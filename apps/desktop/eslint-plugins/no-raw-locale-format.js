/**
 * ESLint rule: route every user-facing number/size/date through the central
 * locale-aware formatter layer; don't hardcode a locale or build a one-off
 * `Intl` formatter in feature code.
 *
 * Rationale: the app has ONE locale source (`$lib/intl/locale.ts`) feeding ONE
 * formatting layer (`$lib/intl/number-format.ts` for numbers/sizes, and the
 * `$lib/settings/format-utils.ts` date formatter). Counts go through
 * `formatInteger`/`formatNumber`, sizes through `formatFileSizeWithFormat`/
 * `formatSizeForDisplay`, dates through `formatDateForDisplay`. A stray
 * `n.toLocaleString('en-US')` or `new Intl.NumberFormat(...)` in feature code
 * re-hardcodes a locale (or silently picks the runtime default), which is the
 * exact drift this layer removes. See `docs/specs/i18n-formatter-layer-plan.md`.
 *
 * What it flags:
 *   - any `<expr>.toLocaleString(...)` call (numbers AND dates), and
 *   - `new Intl.NumberFormat(...)` / `new Intl.DateTimeFormat(...)`.
 * `Intl.Segmenter` / `Intl.Locale` are not formatters and are NOT flagged.
 *
 * Exempt by path: the `lib/intl/` layer itself, the central `format-utils.ts`,
 * and the calendar helper `filter-popover-helpers.ts` (legitimately builds
 * locale-aware weekday/month names; it already reads the chokepoint).
 *
 * Opt out per-line if you truly must:
 *   // eslint-disable-next-line cmdr/no-raw-locale-format -- <reason>
 */

const allowedPathFragments = [
  '/lib/intl/',
  '/lib/settings/format-utils.ts',
  '/lib/query-ui/filter-chips/filter-popover-helpers.ts',
]

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Format user-facing numbers/sizes/dates through `$lib/intl` and the central format utils, not raw `toLocaleString` / `Intl` formatters.',
      recommended: true,
    },
    messages: {
      rawToLocaleString:
        "Don't call `.toLocaleString(...)` in feature code. Route counts through `formatInteger`/`formatNumber` " +
        '(`$lib/intl/number-format`), sizes through `formatSizeForDisplay`, and dates through `formatDateForDisplay`. ' +
        'See `docs/specs/i18n-formatter-layer-plan.md`.',
      rawIntlFormatter:
        "Don't construct `new Intl.{{ ctor }}(...)` in feature code. Use the memoized factory in `$lib/intl/number-format` " +
        'or the date formatter in `$lib/settings/format-utils`, both keyed on the single `getLocale()` source.',
    },
    schema: [],
  },
  create(context) {
    const filename = context.filename || context.getFilename() || ''
    if (allowedPathFragments.some((fragment) => filename.includes(fragment))) {
      return {}
    }

    return {
      CallExpression(node) {
        const callee = node.callee
        if (
          callee.type === 'MemberExpression' &&
          !callee.computed &&
          callee.property.type === 'Identifier' &&
          callee.property.name === 'toLocaleString'
        ) {
          context.report({ node: callee.property, messageId: 'rawToLocaleString' })
        }
      },
      NewExpression(node) {
        const callee = node.callee
        // Match `new Intl.NumberFormat(...)` / `new Intl.DateTimeFormat(...)`.
        if (
          callee.type === 'MemberExpression' &&
          !callee.computed &&
          callee.object.type === 'Identifier' &&
          callee.object.name === 'Intl' &&
          callee.property.type === 'Identifier' &&
          (callee.property.name === 'NumberFormat' || callee.property.name === 'DateTimeFormat')
        ) {
          context.report({
            node: callee,
            messageId: 'rawIntlFormatter',
            data: { ctor: callee.property.name },
          })
        }
      },
    }
  },
}
