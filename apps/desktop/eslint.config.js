/**
 * ESLint Configuration for Cmdr
 *
 * This config uses the flat config format (ESLint 9+) and enforces:
 * - Strict TypeScript type checking for safety
 * - Prettier integration for consistent formatting
 * - Complexity limits (max 15) to keep functions maintainable
 * - No unsafe operations (any, unsafe assignments, etc.)
 *
 * The config is split into multiple sections:
 * 1. Global ignores (dist, build, etc.)
 * 2. TypeScript files (strict type checking)
 * 3. Config files (lighter rules)
 * 4. Svelte files (Svelte-specific parsing + TypeScript)
 *
 * Environment variables:
 * - ESLINT_TYPECHECK_ONLY=1: Run only type-aware rules (requires projectService)
 * - ESLINT_NO_TYPECHECK=1: Run everything except type-aware rules (no projectService)
 * - Neither set: Run all rules (default, full check)
 */
import js from '@eslint/js'
import prettier from 'eslint-plugin-prettier'
import prettierConfig from 'eslint-config-prettier'
import tseslint from 'typescript-eslint'
import svelte from 'eslint-plugin-svelte'
import svelteParser from 'svelte-eslint-parser'
import globals from 'globals'
import noIsolatedTests from './eslint-plugins/no-isolated-tests.js'

/* global process */
const typecheckOnly = process.env.ESLINT_TYPECHECK_ONLY === '1'
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
    'prettier/prettier': 'error',
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
    if (typecheckOnly) {
        // Only type-aware rules; disable everything else from the base config.
        return sharedTypeAwareRules
    }
    if (noTypecheck) {
        // All rules except type-aware ones.
        return { ...sharedNonTypeAwareRules, ...typeAwareRulesOff }
    }
    // Full: all rules.
    return { ...sharedNonTypeAwareRules, ...sharedTypeAwareRules }
}

// projectService config — only enabled when type checking is needed.
const projectServiceConfig =
    noTypecheck ? {} : { projectService: true, tsconfigRootDir: import.meta.dirname }

export default tseslint.config(
    {
        ignores: [
            'dist',
            'build',
            '.svelte-kit',
            'node_modules',
            'src-tauri/target',
            '_ignored',
            // E2E tests use different frameworks (WebDriverIO, Playwright) with their own typing
            'test/e2e-linux',
            'test/e2e-macos',
            'test/e2e-smoke',
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
            prettier,
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
    ...(typecheckOnly
        ? []
        : [
              {
                  // Node.js scripts (like tauri-wrapper.js) need Node globals
                  files: ['scripts/*.js'],
                  plugins: {
                      prettier,
                  },
                  languageOptions: {
                      ecmaVersion: 'latest',
                      sourceType: 'module',
                      globals: {
                          ...globals.node,
                      },
                  },
                  rules: {
                      'prettier/prettier': 'error',
                  },
              },
              {
                  files: ['vite.config.js', 'vitest.config.ts', 'playwright.config.ts'],
                  plugins: {
                      prettier,
                  },
                  languageOptions: {
                      ecmaVersion: 'latest',
                      sourceType: 'module',
                      globals: {
                          ...globals.node,
                      },
                  },
                  rules: {
                      'prettier/prettier': 'error',
                  },
              },
          ]),
    {
        // Svelte 5 runes files (.svelte.ts) - TypeScript with Svelte runes support
        files: ['**/*.svelte.ts'],
        plugins: {
            '@typescript-eslint': tseslint.plugin,
            prettier,
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
        plugins: {
            prettier,
        },
        languageOptions: {
            parser: svelteParser,
            parserOptions: {
                parser: tseslint.parser,
                ...projectServiceConfig,
                extraFileExtensions: ['.svelte'],
            },
        },
        rules: typecheckOnly
            ? {}
            : {
                  'prettier/prettier': 'error',
                  '@typescript-eslint/no-unused-vars': 'error',
                  'no-console': 'warn',
                  complexity: [
                      'error',
                      {
                          max: 15,
                      },
                  ],
              },
    },
    ...(typecheckOnly
        ? []
        : [
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
          ]),
)
