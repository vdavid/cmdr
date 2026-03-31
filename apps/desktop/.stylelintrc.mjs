import { fileURLToPath } from 'url'
import { dirname, join } from 'path'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)

export default {
  extends: ['stylelint-config-standard'],
  plugins: ['stylelint-value-no-unknown-custom-properties', 'stylelint-declaration-block-no-ignored-properties'],
  customSyntax: 'postcss-html',
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
      '/^(padding|margin|gap|row-gap|column-gap)(-\\w+)?$/': ['/\\d+px/'],
      'font-size': ['/\\dpx/'],
      'border-radius': ['/\\dpx/'],
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
    'custom-property-pattern': '^(color|spacing|font|radius|shadow|transition|z)-.+',
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
