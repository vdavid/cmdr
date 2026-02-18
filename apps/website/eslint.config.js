/**
 * ESLint configuration for website
 *
 * Astro + TypeScript checking for the marketing site.
 */
import js from '@eslint/js'
import prettier from 'eslint-plugin-prettier'
import prettierConfig from 'eslint-config-prettier'
import tseslint from 'typescript-eslint'
import astro from 'eslint-plugin-astro'
import globals from 'globals'

export default tseslint.config(
    {
        ignores: ['dist', 'node_modules', '.astro', 'test-results'],
    },
    js.configs.recommended,
    prettierConfig,
    ...astro.configs.recommended,
    {
        files: ['**/*.ts'],
        plugins: {
            '@typescript-eslint': tseslint.plugin,
            prettier,
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
            'prettier/prettier': 'error',
            '@typescript-eslint/no-unused-vars': 'error',
            'no-console': 'warn',
            complexity: ['error', { max: 15 }],
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
            prettier,
        },
        languageOptions: {
            parserOptions: {
                parser: tseslint.parser,
            },
        },
        rules: {
            'prettier/prettier': 'error',
        },
    },
    {
        // Scripts extracted from .astro files by eslint-plugin-astro
        files: ['**/*.astro/*.js', '**/*.astro/*.ts'],
        languageOptions: {
            globals: {
                ...globals.browser,
                Paddle: 'readonly', // Paddle payment SDK loaded via script tag
            },
        },
    },
)
