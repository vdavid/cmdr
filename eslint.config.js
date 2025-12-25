import js from '@eslint/js'
import prettier from 'eslint-plugin-prettier'
import prettierConfig from 'eslint-config-prettier'
import tseslint from 'typescript-eslint'
import svelte from 'eslint-plugin-svelte'
import svelteParser from 'svelte-eslint-parser'
import globals from 'globals'

export default tseslint.config(
    {
        ignores: ['dist', 'build', '.svelte-kit', 'node_modules', 'src-tauri/target'],
    },
    js.configs.recommended,
    prettierConfig,
    ...tseslint.configs.strictTypeChecked.map((config) => ({
        ...config,
        files: ['**/*.{ts,tsx,svelte}'],
    })),
    ...svelte.configs['flat/recommended'],
    {
        files: ['**/*.{ts,tsx}'],
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
            '@typescript-eslint/no-unused-vars': 'error',
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
            complexity: [
                'warn',
                {
                    max: 15,
                },
            ],
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
                'warn',
                {
                    max: 15,
                },
            ],
        },
    },
)
