/**
 * ESLint configuration for website
 *
 * Astro + TypeScript checking for the marketing site.
 */
import js from '@eslint/js'
import prettierConfig from 'eslint-config-prettier'
import tseslint from 'typescript-eslint'
import astro from 'eslint-plugin-astro'
import betterTailwindcss from 'eslint-plugin-better-tailwindcss'
import globals from 'globals'
import noConfusableCallbackParams from '../../eslint-plugins/no-confusable-callback-params.js'

export default tseslint.config(
  {
    ignores: ['dist', 'dist-analytics', 'node_modules', '.astro', 'test-results'],
  },
  js.configs.recommended,
  prettierConfig,
  ...astro.configs.recommended,
  {
    files: ['e2e/**/*.ts'],
    languageOptions: {
      globals: {
        ...globals.browser,
      },
    },
  },
  {
    files: ['**/*.ts'],
    plugins: {
      '@typescript-eslint': tseslint.plugin,
    },
    languageOptions: {
      parser: tseslint.parser,
      ecmaVersion: 'latest',
      sourceType: 'module',
      globals: {
        ...globals.node,
        ...globals.es2021,
      },
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      // Swap the base `no-unused-vars` for the type-aware one: the base rule, run on a TS AST,
      // false-positives on function-type params (e.g. `(e: KeyboardEvent) => void`).
      'no-unused-vars': 'off',
      '@typescript-eslint/no-unused-vars': 'error',
      'no-console': 'warn',
      complexity: ['error', { max: 15 }],
    },
  },
  {
    // Console is the legitimate diagnostic channel in E2E specs (no app context;
    // axe-violation output is the point). Placed after the `**/*.ts` block so it
    // wins for `e2e/**`. Runtime code keeps `no-console: warn`.
    files: ['e2e/**/*.ts'],
    rules: {
      'no-console': 'off',
    },
  },
  {
    files: ['src/dev/**/*.ts'],
    languageOptions: {
      globals: {
        ...globals.browser,
      },
    },
  },
  {
    files: ['**/*.mjs'],
    languageOptions: {
      ecmaVersion: 'latest',
      sourceType: 'module',
      globals: {
        ...globals.node,
        ...globals.es2021,
      },
    },
  },
  {
    files: ['**/*.astro'],
    plugins: {
      '@typescript-eslint': tseslint.plugin,
    },
    languageOptions: {
      parserOptions: {
        parser: tseslint.parser,
      },
    },
    rules: {
      // The astro parser lints the whole `.astro` file as one TS AST (frontmatter + template +
      // `<script>` bodies), so the base `no-unused-vars` from `js.configs.recommended` misfires on
      // function-type params (e.g. `let currentKeydown: ((e: KeyboardEvent) => void) | null`). That
      // misfire is environment-sensitive: it surfaces under the no-TTY check runner / CI but not in a
      // local TTY run. Swap in the type-aware rule, which understands type positions. (Caveat: the
      // type-aware rule covers frontmatter but not extracted client-`<script>` locals; the base rule
      // used to, at the cost of this false positive.)
      'no-unused-vars': 'off',
      '@typescript-eslint/no-unused-vars': 'error',
    },
  },
  {
    // Scripts extracted from .astro files by eslint-plugin-astro. Same base-vs-typed swap.
    files: ['**/*.astro/*.js', '**/*.astro/*.ts'],
    plugins: {
      '@typescript-eslint': tseslint.plugin,
    },
    languageOptions: {
      parser: tseslint.parser,
      globals: {
        ...globals.browser,
        Paddle: 'readonly', // Paddle payment SDK loaded via script tag
      },
    },
    rules: {
      'no-unused-vars': 'off',
      '@typescript-eslint/no-unused-vars': 'error',
    },
  },
  {
    // Tailwind class hygiene. `enforce-canonical-classes` is the ESLint twin of Tailwind IntelliSense's
    // `suggestCanonicalClasses` (set `"tailwindCSS.lint.suggestCanonicalClasses": "ignore"` in your editor so
    // it isn't reported twice). It subsumes `enforce-shorthand-classes`, `enforce-consistent-important-position`,
    // and `enforce-consistent-variable-syntax`, so those stay off. `no-unknown-classes` stays off too: the site
    // mixes Tailwind with BEM classes styled in scoped `<style>` blocks, which the plugin can't see. Boots
    // Tailwind itself, so it adds ~1s of startup. Rationale and the canonical-form contract: `DETAILS.md`.
    files: ['src/**/*.astro', 'src/**/*.ts'],
    plugins: { 'better-tailwindcss': betterTailwindcss },
    // The plugin's default selectors already cover Astro's `class:list` (strings and object keys).
    settings: { 'better-tailwindcss': { entryPoint: 'src/styles/global.css' } },
    rules: {
      'better-tailwindcss/enforce-canonical-classes': 'error',
      'better-tailwindcss/enforce-consistent-class-order': 'error',
      'better-tailwindcss/no-conflicting-classes': 'error',
      'better-tailwindcss/no-deprecated-classes': 'error',
      'better-tailwindcss/no-duplicate-classes': 'error',
      'better-tailwindcss/no-unnecessary-whitespace': 'error',
    },
  },
  {
    // Shared with the desktop app, api-server, and analytics dashboard, which register the
    // same rule under the same name.
    files: ['**/*.ts'],
    plugins: { cmdr: { rules: { 'no-confusable-callback-params': noConfusableCallbackParams } } },
    rules: { 'cmdr/no-confusable-callback-params': 'error' },
  },
)
