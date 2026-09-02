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
      // The house type-aware set, identical in `api-server` and `analytics-dashboard`. The site
      // already builds the TypeScript project for `projectService`, so these cost little beyond
      // what the parse already pays for.
      '@typescript-eslint/no-unsafe-assignment': 'error',
      '@typescript-eslint/no-unsafe-call': 'error',
      '@typescript-eslint/no-unsafe-member-access': 'error',
      '@typescript-eslint/no-unsafe-return': 'error',
      '@typescript-eslint/no-floating-promises': 'error',
      '@typescript-eslint/await-thenable': 'error',
      '@typescript-eslint/no-misused-promises': 'error',
      '@typescript-eslint/require-await': 'error',
      '@typescript-eslint/no-explicit-any': 'error',
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
        // `astro-eslint-parser` doesn't support `projectService` and silently rewrites it to
        // `project: true`, so ask for `project: true` directly and skip the warning on every run.
        project: true,
        tsconfigRootDir: import.meta.dirname,
        extraFileExtensions: ['.astro'],
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
      // The promise family works on `.astro` frontmatter (verified by planting a floating promise
      // and an unawaited async function, 2026-09-02): frontmatter is where `await getCollection()`
      // and `await render(post)` live, so a dropped `await` here renders a half-built page.
      '@typescript-eslint/no-floating-promises': 'error',
      '@typescript-eslint/await-thenable': 'error',
      '@typescript-eslint/no-misused-promises': 'error',
      '@typescript-eslint/require-await': 'error',
      '@typescript-eslint/no-explicit-any': 'error',
      // ❌ Don't add the `no-unsafe-*` family here. Astro's `Astro.props` inference and its template
      // JSX are Volar features that plain TypeScript can't resolve, so those four rules reported 23
      // findings across eight files, every one of them false, while `astro check` found 0 errors in
      // the same files. They read as "type that cannot be resolved" or "type error", not `any`,
      // which is the tell. Nothing in the source can fix them; they'd only buy `eslint-disable`
      // noise. (verified on astro-eslint-parser 1.x + typescript-eslint 8.64, 2026-09-02)
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
