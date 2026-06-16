import { fileURLToPath } from 'url'
import { dirname, join } from 'path'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)

export default {
  extends: ['stylelint-config-standard'],
  plugins: ['stylelint-value-no-unknown-custom-properties', 'stylelint-declaration-block-no-ignored-properties'],
  customSyntax: 'postcss-html',
  // Shrink-wrap for disable comments: a `stylelint-disable` that no longer
  // suppresses anything is an error, so stale opt-outs can't linger.
  reportNeedlessDisables: true,
  overrides: [
    {
      files: ['**/app.css'],
      rules: {
        'color-no-hex': null,
        'function-disallowed-list': null,
      },
    },
  ],
  rules: {
    'color-no-hex': true,
    'function-disallowed-list': ['rgba', 'rgb', 'hsl', 'hsla'],
    'csstools/value-no-unknown-custom-properties': [
      true,
      {
        // Use absolute path to avoid issues when IDE runs stylelint from different directories
        importFrom: [join(__dirname, 'src/app.css')],
      },
    ],
    // Forbid var() with fallback values - all colors should be in app.css
    'declaration-property-value-disallowed-list': {
      '/.*/': ['/var\\(--[\\w-]+\\s*,/'],
      // Ban only raw px values that ALREADY have a design token, i.e. "if a token
      // exists for this exact value, use it." Sub-grid nudges (1px borders, -1px
      // overlaps) and token-less display sizes have no token and stay raw without
      // a disable. Keep these value lists in sync with the --spacing-* /
      // --font-size-* / --radius-* scales in `app.css`.
      '/^(padding|margin|gap|row-gap|column-gap)(-\\w+)?$/': ['/\\b(2|4|8|12|16|24|32)px\\b/'], // --spacing-*
      'font-size': ['/\\b(10|12|14|16|20)px\\b/'], // --font-size-*
      'border-radius': ['/\\b(2|4|6|8|20|29)px\\b/'], // --radius-*
      'z-index': ['/^\\d{2,}/'],
      'font-family': ['/^(?!var\\(|inherit|unset|initial)/'],
      cursor: ['pointer'],
      // --color-accent has insufficient contrast as text on light backgrounds.
      // Use --color-accent-text for foreground text (auto-darkened for a11y).
      color: ['/var\\(--color-accent\\)/'],
    },
    'declaration-no-important': true,
    'declaration-property-value-allowed-list': {
      'font-weight': ['400', '500', '600', 'normal', 'inherit'],
      opacity: ['/^(0|0\\.3|0\\.4|0\\.5|0\\.6|0\\.7|0\\.8|1|inherit)$/'],
    },
    'custom-property-pattern': '^(color|spacing|font|radius|shadow|transition|z|sheet|titlebar)-.+',
    'declaration-block-no-duplicate-custom-properties': true,
    'selector-class-pattern': null,
    'no-descending-specificity': null,
    'color-hex-length': null,
    'color-function-notation': null,
    'alpha-value-notation': null,
    'value-keyword-case': null,
    'property-no-vendor-prefix': null,
    'selector-pseudo-element-colon-notation': null,
    'font-family-no-duplicate-names': null,
    'declaration-property-value-keyword-no-deprecated': null,
    'declaration-block-no-redundant-longhand-properties': null,
    'plugin/declaration-block-no-ignored-properties': true,
    'comment-empty-line-before': null,
    'color-function-alias-notation': null,
    'keyframes-name-pattern': null,
    'rule-empty-line-before': null,
    'comment-whitespace-inside': null,
    'selector-pseudo-class-no-unknown': [
      true,
      {
        ignorePseudoClasses: ['global'],
      },
    ],
    'shorthand-property-no-redundant-values': null,
  },
  ignoreFiles: ['dist/**', 'build/**', '.svelte-kit/**', 'node_modules/**', 'src-tauri/target/**'],
}
