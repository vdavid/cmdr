import { fileURLToPath } from 'url'
import { dirname, join } from 'path'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)

export default {
    extends: ['stylelint-config-standard'],
    plugins: ['stylelint-value-no-unknown-custom-properties'],
    customSyntax: 'postcss-html',
    rules: {
        'csstools/value-no-unknown-custom-properties': [
            true,
            {
                // Use absolute path to avoid issues when IDE runs stylelint from different directories
                importFrom: [join(__dirname, 'src/app.css')],
            },
        ],
        'custom-property-pattern': '^(color|spacing|font)-.+',
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
