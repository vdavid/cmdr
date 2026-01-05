/**
 * ESLint Configuration for Rusty Commander
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
 */
import js from '@eslint/js'
import prettier from 'eslint-plugin-prettier'
import prettierConfig from 'eslint-config-prettier'
import tseslint from 'typescript-eslint'
import svelte from 'eslint-plugin-svelte'
import svelteParser from 'svelte-eslint-parser'
import globals from 'globals'

export default tseslint.config(
    {
        ignores: ['dist', 'build', '.svelte-kit', 'node_modules', 'src-tauri/target', '_ignored'],
    },
    js.configs.recommended,
    prettierConfig,
    ...tseslint.configs.strictTypeChecked.map((config) => ({
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
            parserOptions: {
                projectService: true,
                tsconfigRootDir: import.meta.dirname,
            },
        },
        rules: {
            'prettier/prettier': 'error',
            // Type safety rules - we want to catch all unsafe operations at compile time
            '@typescript-eslint/no-unused-vars': 'error',
            '@typescript-eslint/no-unsafe-assignment': 'error',
            '@typescript-eslint/no-unsafe-call': 'error',
            '@typescript-eslint/no-unsafe-member-access': 'error',
            '@typescript-eslint/no-unsafe-return': 'error',
            // Async/Promise safety - prevent common async bugs
            '@typescript-eslint/no-floating-promises': 'error',
            '@typescript-eslint/await-thenable': 'error',
            '@typescript-eslint/no-misused-promises': 'error',
            '@typescript-eslint/require-await': 'error',
            // Explicit types - 'any' defeats the purpose of TypeScript
            '@typescript-eslint/no-explicit-any': 'error',
            // Code quality - console.log should be removed before commit (warn, not error)
            'no-console': 'warn',
            // Complexity limit - matches Rust clippy's cognitive-complexity-threshold
            complexity: [
                'error',
                {
                    max: 15,
                },
            ],
        },
    },
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
            parserOptions: {
                projectService: true,
                tsconfigRootDir: import.meta.dirname,
            },
        },
        rules: {
            'prettier/prettier': 'error',
            // Type safety rules - same as main TypeScript files
            '@typescript-eslint/no-unused-vars': 'error',
            '@typescript-eslint/no-unsafe-assignment': 'error',
            '@typescript-eslint/no-unsafe-call': 'error',
            '@typescript-eslint/no-unsafe-member-access': 'error',
            '@typescript-eslint/no-unsafe-return': 'error',
            // Async/Promise safety
            '@typescript-eslint/no-floating-promises': 'error',
            '@typescript-eslint/await-thenable': 'error',
            '@typescript-eslint/no-misused-promises': 'error',
            '@typescript-eslint/require-await': 'error',
            // No any
            '@typescript-eslint/no-explicit-any': 'error',
            'no-console': 'warn',
            // Complexity limit
            complexity: ['error', { max: 15 }],
        },
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
                projectService: true,
                tsconfigRootDir: import.meta.dirname,
                extraFileExtensions: ['.svelte'],
            },
        },
        rules: {
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
)
