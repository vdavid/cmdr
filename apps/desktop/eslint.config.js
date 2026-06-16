/**
 * ESLint Configuration for Cmdr
 *
 * This config uses the flat config format (ESLint 9+) and enforces:
 * - Strict TypeScript type checking for safety
 * - Complexity limits (max 15) to keep functions maintainable
 * - No unsafe operations (any, unsafe assignments, etc.)
 *
 * Formatting is handled by oxfmt (not ESLint). eslint-config-prettier disables
 * ESLint rules that would conflict with the formatter.
 *
 * The config is split into multiple sections:
 * 1. Global ignores (dist, build, etc.)
 * 2. TypeScript files (strict type checking)
 * 3. Config files (lighter rules)
 * 4. Svelte files (Svelte-specific parsing + TypeScript)
 *
 * Environment variables:
 * - ESLINT_NO_TYPECHECK=1: Run everything except type-aware rules (no projectService).
 *   Also suppresses reportUnusedDisableDirectives since disable comments for
 *   type-aware rules would look unused. The full run catches stale comments.
 * - Without the env var: Run all rules (default, full check)
 */
import js from '@eslint/js'
import prettierConfig from 'eslint-config-prettier'
import tseslint from 'typescript-eslint'
import svelte from 'eslint-plugin-svelte'
import svelteParser from 'svelte-eslint-parser'
import globals from 'globals'
import noIsolatedTests from './eslint-plugins/no-isolated-tests.js'
import noErrorStringMatch from './eslint-plugins/no-error-string-match.js'
import noRawTauriInvoke from './eslint-plugins/no-raw-tauri-invoke.js'
import noExplorerStateWrites from './eslint-plugins/no-explorer-state-writes.js'
import noRawCommandDispatch from './eslint-plugins/no-raw-command-dispatch.js'
import noRawLucideImport from './eslint-plugins/no-raw-lucide-import.js'
import noRawLocaleFormat from './eslint-plugins/no-raw-locale-format.js'
import dialogNeedsFocusTrap from './eslint-plugins/dialog-needs-focus-trap.js'
import noArbitrarySleepInE2E from './eslint-plugins/no-arbitrary-sleep-in-e2e.js'

/* global process */
const noTypecheck = process.env.ESLINT_NO_TYPECHECK === '1'

// Rules that require TypeScript's project service (type information).
// These are expensive (~45% of lint time) because they need the full type checker.
const typeAwareRuleNames = [
  '@typescript-eslint/no-floating-promises',
  '@typescript-eslint/no-unsafe-assignment',
  '@typescript-eslint/no-misused-promises',
  '@typescript-eslint/no-deprecated',
  '@typescript-eslint/no-unsafe-argument',
  '@typescript-eslint/no-unsafe-return',
  '@typescript-eslint/no-unsafe-call',
  '@typescript-eslint/no-unsafe-member-access',
  '@typescript-eslint/await-thenable',
  '@typescript-eslint/require-await',
  '@typescript-eslint/restrict-template-expressions',
  '@typescript-eslint/no-unnecessary-type-assertion',
  '@typescript-eslint/no-unnecessary-condition',
  '@typescript-eslint/strict-boolean-expressions',
  '@typescript-eslint/no-confusing-void-expression',
  '@typescript-eslint/no-unsafe-enum-comparison',
  '@typescript-eslint/no-base-to-string',
  '@typescript-eslint/no-duplicate-type-constituents',
  '@typescript-eslint/no-redundant-type-constituents',
  '@typescript-eslint/no-meaningless-void-operator',
  '@typescript-eslint/no-mixed-enums',
  '@typescript-eslint/no-unnecessary-boolean-literal-compare',
  '@typescript-eslint/no-unnecessary-template-expression',
  '@typescript-eslint/no-unnecessary-type-parameters',
  '@typescript-eslint/prefer-reduce-type-parameter',
  '@typescript-eslint/prefer-return-this-type',
  '@typescript-eslint/unified-signatures',
  '@typescript-eslint/use-unknown-in-catch-callback-variable',
  '@typescript-eslint/only-throw-error',
  '@typescript-eslint/prefer-promise-reject-errors',
  '@typescript-eslint/return-await',
  '@typescript-eslint/unbound-method',
]

// Build an object that disables all type-aware rules.
const typeAwareRulesOff = Object.fromEntries(typeAwareRuleNames.map((name) => [name, 'off']))

// Use strict (non-type-checked) base when skipping type checking,
// strictTypeChecked when running type-aware rules.
const tsBaseConfigs = noTypecheck ? tseslint.configs.strict : tseslint.configs.strictTypeChecked

// Shared non-type-aware rules used across TS and Svelte-runes file sections.
const sharedNonTypeAwareRules = {
  '@typescript-eslint/no-unused-vars': 'error',
  '@typescript-eslint/no-explicit-any': 'error',
  'no-console': 'warn',
  complexity: ['error', { max: 15 }],
}

// Shared type-aware rules used across TS and Svelte-runes file sections.
const sharedTypeAwareRules = {
  '@typescript-eslint/no-unsafe-assignment': 'error',
  '@typescript-eslint/no-unsafe-call': 'error',
  '@typescript-eslint/no-unsafe-member-access': 'error',
  '@typescript-eslint/no-unsafe-return': 'error',
  '@typescript-eslint/no-floating-promises': 'error',
  '@typescript-eslint/await-thenable': 'error',
  '@typescript-eslint/no-misused-promises': 'error',
  '@typescript-eslint/require-await': 'error',
}

// Build rule sets based on mode.
function buildTsRules() {
  if (noTypecheck) {
    // All rules except type-aware ones.
    return { ...sharedNonTypeAwareRules, ...typeAwareRulesOff }
  }
  // Full: all rules.
  return { ...sharedNonTypeAwareRules, ...sharedTypeAwareRules }
}

// projectService config: only enabled when type checking is needed.
const projectServiceConfig = noTypecheck ? {} : { projectService: true, tsconfigRootDir: import.meta.dirname }

export default tseslint.config(
  // The fast check skips type-aware rules, so eslint-disable comments targeting
  // those rules look "unused." Suppress that: the full (slow) run catches stale comments.
  ...(noTypecheck ? [{ linterOptions: { reportUnusedDisableDirectives: 'off' } }] : []),
  {
    ignores: [
      'dist',
      'build',
      '.svelte-kit',
      'node_modules',
      'src-tauri/target',
      '_ignored',
      // E2E tests use different frameworks (Playwright) with their own typing
      'test/e2e-linux',
      'test/e2e-smoke',
      // Auto-generated by tauri-specta: do not lint
      'src/lib/ipc/bindings.ts',
    ],
  },
  js.configs.recommended,
  prettierConfig,
  ...tsBaseConfigs.map((config) => ({
    ...config,
    files: ['**/*.{ts,tsx,svelte.ts,svelte}'],
  })),
  ...svelte.configs['flat/recommended'],
  {
    files: ['**/*.{ts,tsx,svelte.ts}'],
    plugins: {
      '@typescript-eslint': tseslint.plugin,
    },
    languageOptions: {
      ecmaVersion: 'latest',
      sourceType: 'module',
      globals: {
        ...globals.browser,
        ...globals.node,
        ...globals.es2021,
      },
      parserOptions: projectServiceConfig,
    },
    rules: buildTsRules(),
  },
  {
    // Node.js scripts (like tauri-wrapper.js) and config files need Node globals
    files: ['scripts/*.js', 'vite.config.js', 'vitest.config.ts', 'playwright.config.ts'],
    languageOptions: {
      ecmaVersion: 'latest',
      sourceType: 'module',
      globals: {
        ...globals.node,
      },
    },
  },
  {
    // Svelte 5 runes files (.svelte.ts) - TypeScript with Svelte runes support
    files: ['**/*.svelte.ts'],
    plugins: {
      '@typescript-eslint': tseslint.plugin,
    },
    languageOptions: {
      parser: tseslint.parser,
      ecmaVersion: 'latest',
      sourceType: 'module',
      globals: {
        ...globals.browser,
        ...globals.node,
        ...globals.es2021,
      },
      parserOptions: projectServiceConfig,
    },
    rules: buildTsRules(),
  },
  {
    files: ['**/*.svelte'],
    languageOptions: {
      parser: svelteParser,
      parserOptions: {
        parser: tseslint.parser,
        ...projectServiceConfig,
        extraFileExtensions: ['.svelte'],
      },
    },
    rules: {
      '@typescript-eslint/no-unused-vars': 'error',
      'no-console': 'warn',
      complexity: ['error', { max: 15 }],
    },
  },
  {
    // Test files - ensure they actually test source code
    // Excludes e2e tests (Playwright) which test through the browser, not via imports
    files: ['src/**/*.test.ts'],
    plugins: {
      custom: {
        rules: {
          'no-isolated-tests': noIsolatedTests,
        },
      },
    },
    rules: {
      'custom/no-isolated-tests': 'error',
    },
  },
  {
    // String-matching error/state values is fragile (couples to wording that's
    // free to change). Push toward typed flags from the backend struct. Mirrors
    // the Rust-side `error-string-match` check in `scripts/check/checks/`.
    // Raw `invoke('name', ...)` is banned outside `lib/ipc/` for the same reason:
    // command names should be type-checked, not magic strings.
    // Assigning to the explorer store's surface is banned outside the store
    // module: state changes go through its named mutators (invariant A2).
    // Dispatching a raw command-id string literal is banned: the bus carries
    // `CommandId`-typed values so renames are caught at compile time (A3).
    // A static dialog-role element without `use:trapFocus` is banned: Tab would
    // walk focus into the suppressed background and lock out the keyboard.
    files: ['src/**/*.{ts,svelte.ts,svelte}', 'src/**/*.test.ts'],
    plugins: {
      cmdr: {
        rules: {
          'no-error-string-match': noErrorStringMatch,
          'no-raw-tauri-invoke': noRawTauriInvoke,
          'no-explorer-state-writes': noExplorerStateWrites,
          'no-raw-command-dispatch': noRawCommandDispatch,
          'no-raw-lucide-import': noRawLucideImport,
          'no-raw-locale-format': noRawLocaleFormat,
          'dialog-needs-focus-trap': dialogNeedsFocusTrap,
        },
      },
    },
    rules: {
      'cmdr/no-error-string-match': 'error',
      'cmdr/no-raw-tauri-invoke': 'error',
      'cmdr/no-explorer-state-writes': 'error',
      'cmdr/no-raw-command-dispatch': 'error',
      'cmdr/no-raw-lucide-import': 'error',
      // One locale source, one formatting layer: feature code routes every
      // user-facing number/size/date through `$lib/intl` and the central format
      // utils, never a raw `.toLocaleString(...)` or a hand-built `Intl.*`
      // formatter. Turned OFF for `*.test.ts` below (tests legitimately
      // construct `Intl.NumberFormat`/`DateTimeFormat` to compute expecteds).
      'cmdr/no-raw-locale-format': 'error',
      'cmdr/dialog-needs-focus-trap': 'error',
    },
  },
  {
    // E2E specs must not use `await sleep(N)`: fixed sleeps are either too
    // tight (flake) or too loose (slow). Use `pollUntil` / `waitForSelector`
    // instead. Helper files (helpers.ts, conflict-helpers.ts, mcp-client.ts)
    // are excluded because `pollUntil` itself calls `sleep(interval)` between
    // iterations. See `docs/testing.md` § "❌ `await sleep(N)` in E2E specs".
    files: ['test/e2e-playwright/**/*.spec.ts'],
    plugins: {
      cmdr: {
        rules: {
          'no-arbitrary-sleep-in-e2e': noArbitrarySleepInE2E,
        },
      },
    },
    rules: {
      'cmdr/no-arbitrary-sleep-in-e2e': 'error',
    },
  },
  {
    // Test code legitimately does things runtime code shouldn't, so relax a few
    // rules here instead of scattering per-line `eslint-disable` across the suite:
    // - no-console: no app logger in a Playwright/Vitest context; the output is the
    //   point (axe violations, fixture lifecycle, skip reasons).
    // - only-throw-error: mockIPC must throw the raw wire/typed-error shape to
    //   exercise the IPC contract.
    // - no-dynamic-delete: fixture resets delete dynamic keys off mock objects.
    // Runtime code keeps all three on (console routes through `getAppLogger`).
    files: ['test/**', 'src/**/*.test.ts', 'src/lib/test-a11y.ts'],
    rules: {
      'no-console': 'off',
      '@typescript-eslint/only-throw-error': 'off',
      '@typescript-eslint/no-dynamic-delete': 'off',
      // Tests construct `Intl.*` formatters to compute expected values, on purpose.
      'cmdr/no-raw-locale-format': 'off',
    },
  },
)
