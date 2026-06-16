/**
 * ESLint configuration for website
 *
 * Astro + TypeScript checking for the marketing site.
 */
import js from '@eslint/js'
import prettierConfig from 'eslint-config-prettier'
import tseslint from 'typescript-eslint'
import astro from 'eslint-plugin-astro'
import globals from 'globals'

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
)
