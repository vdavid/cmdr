/**
 * ESLint rule: route every byte size, transfer rate, and duration through
 * `$lib/units`; don't reimplement the unit ladder privately.
 *
 * Rationale: four private byte formatters once drifted apart in this codebase.
 * Each hardcoded base 1024 while labelling the result "KB" / "MB" / "GB",
 * ignoring the user's `appearance.fileSizeFormat`, which is how two Cmdr
 * windows came to show different numbers for the same transfer. The canonical
 * surface is `$lib/units` (`formatByteSize`, `formatByteRate`,
 * `formatDuration`, `formatMilliseconds`, `formatFilesPerSecond`), with
 * `$lib/ui/Size.svelte` as the component form.
 *
 * What it flags, both of which are the shape a private formatter takes:
 *   - a BINARY unit-ladder literal (`1024`, `1048576`, `1073741824`,
 *     `1099511627776`) as an operand of `*`, `/`, or `**`, and
 *   - a function DECLARATION whose name reads as a unit formatter
 *     (`formatBytes`, `formatSize`, `formatSpeed`, `formatEta`, …) AND whose
 *     body contains a ladder literal or a unit label.
 *
 * Deliberately NOT flagged: a bare `1024` comparison (`if (bytes < 1024)`), a
 * buffer or chunk size, and — importantly — base-1000 arithmetic. `1000` is
 * milliseconds-per-second far more often than it is a kilobyte, and there's no
 * way to tell from the AST; the name-plus-body check covers SI formatters
 * instead. A named formatter that just DELEGATES to `$lib/units` is fine, since
 * its body carries no ladder or label.
 *
 * Exempt by path: `lib/units/` (the implementation), the size-tier layer in
 * `file-explorer/selection/selection-info-utils.ts`, and test files (fixtures
 * legitimately spell out `1024 * 1024`).
 *
 * Opt out per-line when a threshold genuinely is a fixed binary constant:
 *   // eslint-disable-next-line cmdr/no-private-unit-format -- <reason>
 */

const allowedPathFragments = [
  '/lib/units/',
  '/lib/file-explorer/selection/selection-info-utils.ts',
  '/lib/intl/number-format.ts',
]

/** Names that read as "I format a size / rate / duration myself". */
const FORMATTER_NAME_RE =
  /^(format|render|humanize|pretty)(Bytes|Byte|Size|Sizes|FileSize|DiskSize|DbSize|Speed|Rate|Throughput|Duration|Eta|Elapsed|Ms|Milliseconds|Seconds)/

/** Binary unit-ladder constants. Base-1000 is excluded on purpose (see above). */
const LADDER_VALUES = new Set([1024, 1048576, 1073741824, 1099511627776])

/** Unit labels a formatter emits. Paired with a formatter-ish NAME, they're the tell. */
const UNIT_LABEL_RE = /['"`]\s*(bytes?|[kKMGTP]i?B|[kKMGTP]i?B\/s|B\/s|ms|sec|secs|seconds)\s*['"`]/

/** Arithmetic operators that turn a ladder constant into a unit conversion. */
const CONVERSION_OPERATORS = new Set(['*', '/', '**'])

function isTestFile(filename) {
  return /\.(test|spec)\.[tj]s$/.test(filename) || filename.includes('/test/')
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Format byte sizes, transfer rates, and durations through `$lib/units`, not a private unit ladder.',
      recommended: true,
    },
    messages: {
      privateLadder:
        "Don't build a unit ladder from `{{ value }}` here. Sizes go through `formatByteSize` / `<Size>`, rates " +
        'through `formatByteRate`, durations through `formatDuration` (all `$lib/units`), so they follow the ' +
        "user's binary/SI setting. If this really is a fixed binary threshold, add " +
        '`// eslint-disable-next-line cmdr/no-private-unit-format -- <reason>`.',
      privateFormatter:
        "`{{ name }}` looks like a private size/rate/duration formatter. Use `$lib/units` instead: that's the one " +
        'place a byte count, a rate, or a duration becomes text, and duplicating it is how two windows came to ' +
        'show different numbers for the same transfer.',
    },
    schema: [],
  },
  create(context) {
    const filename = context.filename || context.getFilename() || ''
    if (allowedPathFragments.some((fragment) => filename.includes(fragment)) || isTestFile(filename)) {
      return {}
    }

    const sourceCode = context.sourceCode ?? context.getSourceCode()

    /**
     * Report a function whose NAME reads as a unit formatter and whose BODY
     * actually does unit work. The body check is what lets a wrapper that
     * delegates to `$lib/units` keep a descriptive name.
     */
    const checkName = (node, name) => {
      if (typeof name !== 'string' || !FORMATTER_NAME_RE.test(name)) return
      const body = sourceCode.getText(node)
      const doesUnitWork = [...LADDER_VALUES].some((v) => new RegExp(`\\b${String(v)}\\b`).test(body))
      if (!doesUnitWork && !UNIT_LABEL_RE.test(body)) return
      context.report({ node, messageId: 'privateFormatter', data: { name } })
    }

    return {
      Literal(node) {
        if (typeof node.value !== 'number' || !LADDER_VALUES.has(node.value)) return
        const parent = node.parent
        if (parent?.type !== 'BinaryExpression' || !CONVERSION_OPERATORS.has(parent.operator)) return
        context.report({ node, messageId: 'privateLadder', data: { value: String(node.value) } })
      },
      FunctionDeclaration(node) {
        checkName(node, node.id?.name)
      },
      VariableDeclarator(node) {
        if (node.init?.type !== 'ArrowFunctionExpression' && node.init?.type !== 'FunctionExpression') return
        if (node.id.type !== 'Identifier') return
        checkName(node, node.id.name)
      },
    }
  },
}
