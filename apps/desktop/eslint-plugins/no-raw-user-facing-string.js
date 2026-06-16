/**
 * ESLint rule: no hardcoded user-facing strings in migrated feature areas.
 * Route them through the i18n catalog (`t()` / `<Trans>` from `$lib/intl`)
 * instead, so the app is translation-ready and copy lives in one place.
 *
 * ## Scope: a CLOSED set of sink positions, not "any string literal"
 *
 * Detecting "any user-facing string" is open-ended and false-positive-ridden
 * (log lines, IPC keys, CSS classes, `data-*`, command ids, role values, test
 * strings all look like copy to a naive scan). So, like the sibling
 * `no-raw-locale-format`, this rule keys on KNOWN sinks only:
 *
 *  - specific component/element props: `title`, `label`, `placeholder`,
 *    `aria-label` (string-literal attribute values in `.svelte` markup),
 *  - `addToast(...)` first argument (the toast content), in `.ts`/`.svelte.ts`,
 *  - text nodes in `.svelte` markup (the rendered copy between tags).
 *
 * Everything else is left alone. A missed user-facing string in an
 * unrecognized position can slip through; this rule is a strong ratchet, not a
 * completeness proof (see the M1/M3 honesty caveat in the plan).
 *
 * ## Scope: an AREA allowlist, widened per migrated tranche
 *
 * The full ~1,000-string migration lands by area (M2). Enforcing the rule
 * everywhere at once would flood every un-migrated component. So the rule fires
 * only for files whose path matches an enforced fragment, MINUS an explicit
 * exclusion list of files in an enforced area that aren't migrated yet. Each M2
 * tranche adds an area fragment AND removes that area's files from the exclusion
 * list as it migrates them (then adds `settings`, `errors`, …).
 *
 * M1 enforces the `transfer` area, with its still-raw DIALOG files excluded: the
 * migrated pilot (`transfer-complete-toast.ts`) is enforced, while the transfer
 * dialogs (~57 raw strings, plus `TransferErrorDialog` which overlaps the errors
 * pipeline) stay excluded until their M2 tranche. The exclusion list IS the
 * remaining-work ledger for the transfer area.
 *
 * Opt out per-line for a genuinely non-copy literal in an enforced area:
 *
 *   // eslint-disable-next-line cmdr/no-raw-user-facing-string -- <reason>
 */

// Path fragments of areas where the rule is enforced. Add a fragment when a
// tranche starts migrating that area (M2). Keep this list and the catalog areas
// in step; an area isn't "done" until its excluded files (below) are all gone.
const enforcedAreaPathFragments = [
  '/lib/file-operations/transfer/', // the `transfer` area (M0/M1 pilot)
  '/lib/settings/', // the `settings` area (M2 tranche 2)
]

// Files inside an enforced area that aren't migrated yet, so the rule skips them
// to avoid flooding the build with known-pending violations. Each M2 tranche
// deletes its entries here as it migrates the file's copy. When this list is
// empty for an area, that area is fully enforced.
const excludedUnmigratedFiles = [
  // The transfer dialogs: a later M2 tranche. `TransferErrorDialog` +
  // `FallbackErrorContent` overlap the errors pipeline (the trickiest tranche).
  '/lib/file-operations/transfer/TransferDialog.svelte',
  '/lib/file-operations/transfer/TransferProgressDialog.svelte',
  '/lib/file-operations/transfer/TransferErrorDialog.svelte',
  '/lib/file-operations/transfer/FallbackErrorContent.svelte',
  '/lib/file-operations/transfer/ScanPhaseBody.svelte',
  '/lib/file-operations/transfer/DirectionIndicator.svelte',

  // Settings area: the registry-driven settings core + section chrome IS
  // migrated and enforced. These four files render copy owned by ADJACENT
  // subsystems (their strings aren't in the settings registry) and migrate with
  // those subsystems' tranches, not this one:
  //   - the cloud/local AI provider config UIs (API-key flows, model picker,
  //     RAM gauge, download progress) belong with the AI tranche,
  //   - keyboard-shortcuts renders command names from the command registry (the
  //     `commands` tranche) plus its own conflict-banner chrome,
  //   - license renders license-API display copy (the licensing surface).
  '/lib/settings/sections/AiCloudSection.svelte',
  '/lib/settings/sections/AiLocalSection.svelte',
  '/lib/settings/sections/KeyboardShortcutsSection.svelte',
  '/lib/settings/sections/LicenseSection.svelte',
]

// Element/component attributes that carry user-facing copy.
const userFacingAttributes = new Set(['title', 'label', 'placeholder', 'aria-label'])

// A string literal that's whitespace/punctuation-only isn't copy worth
// flagging (a separator, a space). Require at least one letter.
function looksLikeCopy(text) {
  return /[A-Za-z]/.test(text)
}

// Elements whose text content is code, not user copy: a `<style>` block holds
// CSS and `<script>` holds JS. The svelte-eslint parser emits the raw CSS/JS as
// a `SvelteText` child of a dedicated `SvelteStyleElement`/`SvelteScriptElement`
// wrapper, so without this guard the rule flags every stylesheet.
const NON_COPY_ELEMENT_TYPES = new Set(['SvelteStyleElement', 'SvelteScriptElement'])

/** Whether a `SvelteText` node sits directly inside a `<style>`/`<script>`. */
function isInNonCopyElement(node) {
  return node.parent ? NON_COPY_ELEMENT_TYPES.has(node.parent.type) : false
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Route user-facing strings in migrated areas through the i18n catalog (`t()` / `<Trans>`), not raw literals in known sinks.',
      recommended: true,
    },
    messages: {
      rawUserFacingString:
        "Don't hardcode a user-facing string here. Move the copy into a `messages/en/<area>.json` catalog key and " +
        'resolve it with `t()` (or `<Trans>` for inline components) from `$lib/intl`. ' +
        'See `src/lib/intl/CLAUDE.md`.',
    },
    schema: [],
  },
  create(context) {
    const filename = context.filename || context.getFilename() || ''
    const inEnforcedArea = enforcedAreaPathFragments.some((fragment) => filename.includes(fragment))
    const isExcluded = excludedUnmigratedFiles.some((fragment) => filename.includes(fragment))
    if (!inEnforcedArea || isExcluded) {
      return {}
    }

    function reportLiteral(node, value) {
      if (typeof value === 'string' && looksLikeCopy(value)) {
        context.report({ node, messageId: 'rawUserFacingString' })
      }
    }

    return {
      // `addToast('...')`: the toast content is the first argument.
      CallExpression(node) {
        const callee = node.callee
        const isAddToast =
          (callee.type === 'Identifier' && callee.name === 'addToast') ||
          (callee.type === 'MemberExpression' &&
            !callee.computed &&
            callee.property.type === 'Identifier' &&
            callee.property.name === 'addToast')
        if (!isAddToast) return
        const firstArg = node.arguments[0]
        if (firstArg && firstArg.type === 'Literal') {
          reportLiteral(firstArg, firstArg.value)
        }
      },

      // `<el title="...">` / `aria-label` / `label` / `placeholder` with a
      // single static string-literal value.
      SvelteAttribute(node) {
        if (!userFacingAttributes.has(node.key.name)) return
        const value = node.value
        if (value.length === 1 && value[0].type === 'SvelteLiteral') {
          reportLiteral(node, value[0].value)
        }
      },

      // Rendered text between tags (`<p>Copy</p>`). The Svelte parser emits a
      // `SvelteText` node; `{expr}` interpolation is a separate mustache node,
      // so a localized `{t('...')}` doesn't reach here.
      SvelteText(node) {
        if (isInNonCopyElement(node)) return
        reportLiteral(node, node.value)
      },
    }
  },
}
