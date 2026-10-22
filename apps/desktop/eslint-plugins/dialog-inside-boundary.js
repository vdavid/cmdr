/**
 * ESLint rule: every dialog a route mounts must sit inside a `DialogBoundary`.
 *
 * Rationale: a dialog that throws while rendering or tearing down blanks the
 * whole window unless an error boundary catches it. Svelte routes a teardown
 * error only to a LIVE ancestor boundary, so a boundary inside the dialog (or
 * inside the `{#if}` that mounts it) dies with the dialog and catches nothing.
 * `DialogBoundary` (`#lib/ui/boundary/DialogBoundary.svelte`) has to sit ABOVE the
 * `{#if}`, where it outlives the dialog and can close it on a throw. This rule
 * makes a new route dialog unable to forget it.
 *
 * ## What this rule catches
 *
 * In a `.svelte` file under `src/routes/` (but not `src/routes/dev/`), a
 * component element whose name (the last segment of a dotted name) ends in
 * `Dialog`, `Dialogs`, `Modal`, `Sheet`, `Palette`, `Prompt`, `Wizard`, or
 * `Window`, with no `DialogBoundary` component among its ancestors. Elements,
 * `{#if}`, `{#key}`, `{#each}`, and snippets may sit in between.
 *
 * ## What it deliberately does NOT catch
 *
 * - Dialogs outside `src/routes/`: a dialog composing another dialog is covered
 *   by the boundary the route already put around the outer one.
 * - `src/routes/dev/`: the dev-only component catalog mounts dialogs for display.
 * - A route file that is itself a dialog (its basename ends in one of the
 *   suffixes above, like `ViewerCopyDialogs.svelte`): its PARENT is the file
 *   that must wrap it, and the boundary there covers the dialogs inside.
 * - Dialogs in a top-level `{#snippet}` that's `{@render}`ed inside a boundary:
 *   the check is a plain ancestor walk, so these get reported. Move the snippet
 *   inside the boundary, or wrap at the definition.
 * - Dialog components whose names don't end in one of the suffixes above.
 *
 * Opt out per-element for a surface that genuinely can't sit in a boundary:
 *
 *   <!-- eslint-disable-next-line cmdr/dialog-inside-boundary -- <reason> -->
 */

const DIALOG_NAME = /(Dialogs?|Modal|Sheet|Palette|Prompt|Wizard|Window)$/
const BOUNDARY_NAME = 'DialogBoundary'

/** Normalizes separators so the path checks work on Windows filenames too. */
function isInScope(filename) {
  const path = filename.replaceAll('\\', '/')
  if (!path.endsWith('.svelte') || !path.includes('/src/routes/') || path.includes('/src/routes/dev/')) return false
  // A file that is itself a dialog is wrapped by its parent, which this rule checks.
  const basename = path.slice(path.lastIndexOf('/') + 1, -'.svelte'.length)
  return !DIALOG_NAME.test(basename)
}

/** The component's tag name, reduced to its last segment (`Foo.BarDialog` → `BarDialog`). */
function componentName(node, sourceCode) {
  const fullName = sourceCode.getText(node.name)
  return fullName.slice(fullName.lastIndexOf('.') + 1)
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description: 'Require every dialog a route mounts to sit inside a `DialogBoundary`.',
      recommended: true,
    },
    messages: {
      missingBoundary:
        '`<{{ name }}>` has no `DialogBoundary` above it, so a throw while it renders or tears down blanks ' +
        'the whole window. Wrap the `{#if}` that mounts it in `<DialogBoundary name=… onClose=…>` from ' +
        '`#lib/ui/boundary/DialogBoundary.svelte` (the boundary must sit above the `{#if}`, not inside it). See ' +
        '`apps/desktop/src/DETAILS.md` § "Error boundaries".',
    },
    schema: [],
  },
  create(context) {
    if (!isInScope(context.filename)) return {}
    const sourceCode = context.sourceCode

    return {
      SvelteElement(node) {
        if (node.kind !== 'component') return
        const name = componentName(node, sourceCode)
        if (name === BOUNDARY_NAME || !DIALOG_NAME.test(name)) return

        for (let ancestor = node.parent; ancestor; ancestor = ancestor.parent) {
          if (
            ancestor.type === 'SvelteElement' &&
            ancestor.kind === 'component' &&
            componentName(ancestor, sourceCode) === BOUNDARY_NAME
          ) {
            return
          }
        }

        // Report on the start tag so an `<!-- eslint-disable-next-line ... -->`
        // comment right above the element can suppress it.
        context.report({
          node: node.startTag,
          messageId: 'missingBoundary',
          data: { name },
        })
      },
    }
  },
}
