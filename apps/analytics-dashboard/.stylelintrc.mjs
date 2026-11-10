/**
 * Stylelint configuration for the analytics dashboard.
 *
 * Much lighter than the desktop app's: this dashboard styles with Tailwind v4 utilities, so there
 * are few hand-written declarations to police and no design-token ladder to enforce. What's left is
 * catching real CSS mistakes in `app.css` and in Svelte `<style>` blocks.
 */
export default {
  extends: ['stylelint-config-standard'],
  plugins: ['stylelint-declaration-block-no-ignored-properties'],
  // `postcss-html` is what lets stylelint see inside `.svelte` `<style>` blocks. Scoped to `.svelte`:
  // set for every file, it parses `app.css` as HTML, finds no `<style>`, and checks nothing (#1056).
  overrides: [{ files: ['**/*.svelte'], customSyntax: 'postcss-html' }],
  // A `stylelint-disable` that no longer suppresses anything is an error, so stale opt-outs can't
  // linger. Same principle as the desktop config and knip's `treatConfigHintsAsErrors`.
  reportNeedlessDisables: true,
  rules: {
    // Tailwind v4 is CSS-first: `@theme`, `@utility`, and friends are its config surface, and
    // `stylelint-config-standard` would otherwise reject every one of them as unknown.
    'at-rule-no-unknown': [
      true,
      {
        ignoreAtRules: ['theme', 'utility', 'variant', 'custom-variant', 'apply', 'source', 'reference', 'plugin'],
      },
    ],
    'at-rule-no-deprecated': [true, { ignoreAtRules: ['apply'] }],
    // Svelte's `:global()` is not a real pseudo-class.
    'selector-pseudo-class-no-unknown': [true, { ignorePseudoClasses: ['global'] }],
    'declaration-no-important': true,
    'plugin/declaration-block-no-ignored-properties': true,
    'declaration-block-no-duplicate-custom-properties': true,
    // Tailwind's theme tokens are hex by design and live in `@theme`; the rest of the app uses
    // `var(--color-*)`, so hex outside `app.css` would be the smell, not hex inside it.
    'custom-property-pattern': '^color-.+',
    // `@import 'tailwindcss'` is Tailwind v4's documented entry form; the standard config wants `url()`.
    'import-notation': 'string',
    // Blank lines group the `@theme` tokens (surfaces, text, accent, status); the standard config forbids them.
    'custom-property-empty-line-before': null,
    // Utility-class names come from Tailwind, not from us.
    'selector-class-pattern': null,
    'no-descending-specificity': null,
    'color-hex-length': null,
    'color-function-notation': null,
    'alpha-value-notation': null,
    'value-keyword-case': null,
  },
  ignoreFiles: ['build/**', '.svelte-kit/**', 'node_modules/**'],
}
