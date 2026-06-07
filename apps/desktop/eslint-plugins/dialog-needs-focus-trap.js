/**
 * ESLint rule: every dialog-role element must carry `use:trapFocus`.
 *
 * Rationale: `aria-modal="true"` is assistive-tech semantics only — it does NOT
 * stop the browser from tabbing focus out of the overlay. A dialog without a
 * focus trap lets Tab walk into the (inert-looking) background, where the app's
 * global shortcut dispatch is suppressed while the dialog flag is up, locking
 * the user out of the keyboard entirely (the command-palette Tab-Tab lockout).
 * The shared `use:trapFocus` action (`$lib/ui/focus-trap.ts`) wraps Tab, pulls
 * leaked focus back, and keeps Escape working; this rule makes new dialogs
 * unable to forget it.
 *
 * ## What this rule catches
 *
 * A Svelte element with a STATIC `role="dialog"` or `role="alertdialog"`
 * attribute that doesn't also have a `use:trapFocus` directive on the same
 * element.
 *
 * ## What it deliberately does NOT catch
 *
 * - Dynamic roles (`{role}`, `role={x}`): can't be resolved statically.
 *   `ModalDialog.svelte` is the one such site and it carries the trap.
 * - Elements rendered by a component that has the trap inside (consumers of
 *   `ModalDialog` don't repeat the directive — the primitive owns it).
 *
 * Opt out per-element for genuinely non-modal dialog-role surfaces:
 *
 *   <!-- eslint-disable-next-line cmdr/dialog-needs-focus-trap -- <reason> -->
 */

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description: 'Require `use:trapFocus` on every element with a static dialog role.',
      recommended: true,
    },
    messages: {
      missingTrap:
        'This `role="{{ role }}"` element has no `use:trapFocus`, so Tab can walk focus out into the ' +
        'background and lock the user out of the keyboard. Add `use:trapFocus` from `$lib/ui/focus-trap`, ' +
        'passing your close callback as `onEscape` (omit it only for dialogs that must swallow Escape, like ' +
        'the onboarding wizard). See `lib/ui/CLAUDE.md` § "Focus trapping".',
    },
    schema: [],
  },
  create(context) {
    return {
      SvelteElement(node) {
        if (node.kind !== 'html') return
        const attributes = node.startTag.attributes

        const roleAttribute = attributes.find(
          (attribute) => attribute.type === 'SvelteAttribute' && attribute.key.name === 'role',
        )
        if (!roleAttribute) return

        // Only a single static text chunk counts; `{role}` / `role={x}` are dynamic.
        const value = roleAttribute.value
        const staticRole = value.length === 1 && value[0].type === 'SvelteLiteral' ? value[0].value : undefined
        if (staticRole !== 'dialog' && staticRole !== 'alertdialog') return

        const hasTrap = attributes.some(
          (attribute) =>
            attribute.type === 'SvelteDirective' &&
            attribute.kind === 'Action' &&
            attribute.key.name.type === 'Identifier' &&
            attribute.key.name.name === 'trapFocus',
        )
        if (hasTrap) return

        // Report on the start tag (not the role attribute) so an
        // `<!-- eslint-disable-next-line ... -->` comment right above the
        // element can suppress it — comments can't live inside a tag.
        context.report({
          node: node.startTag,
          messageId: 'missingTrap',
          data: { role: staticRole },
        })
      },
    }
  },
}
