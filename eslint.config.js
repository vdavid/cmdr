import js from '@eslint/js'
import globals from 'globals'
import reactHooks from 'eslint-plugin-react-hooks'
import reactRefresh from 'eslint-plugin-react-refresh'
import react from 'eslint-plugin-react'
import reactX from 'eslint-plugin-react-x'
import reactDom from 'eslint-plugin-react-dom'
import prettier from 'eslint-plugin-prettier'
import prettierConfig from 'eslint-config-prettier'
import tseslint from 'typescript-eslint'
import importPlugin from 'eslint-plugin-import'

export default [
    {
        ignores: ['dist'],
    },
    js.configs.recommended,
    prettierConfig,
    ...tseslint.configs.strictTypeChecked.map(config => ({
        ...config,
        files: ['**/*.{ts,tsx}'],
    })),
    {
        ...reactX.configs['recommended-typescript'],
        files: ['**/*.{ts,tsx}'],
    },
    {
        ...reactDom.configs.recommended,
        files: ['**/*.{ts,tsx}'],
    },
    {
        files: ['**/*.{ts,tsx}'],
        plugins: {
            '@typescript-eslint': tseslint.plugin,
            react,
            'react-hooks': reactHooks,
            'react-refresh': reactRefresh,
            import: importPlugin,
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
                ecmaFeatures: {
                    jsx: true,
                },
                project: ['./tsconfig.node.json', './tsconfig.app.json', './tsconfig.e2e.json'],
                tsconfigRootDir: import.meta.dirname,
            },
        },
        settings: {
            react: {
                version: 'detect',
            },
        },
        rules: {
            ...reactHooks.configs.recommended.rules,
            'react/react-in-jsx-scope': 'off',
            'react/jsx-key': 'error',
            'react/jsx-no-duplicate-props': 'error',
            'react/jsx-no-undef': 'error',
            'react/jsx-uses-react': 'off',
            'react/jsx-uses-vars': 'error',
            'react/no-array-index-key': 'warn',
            'react/no-danger': 'warn',
            'react/no-deprecated': 'error',
            'react/no-direct-mutation-state': 'error',
            'react/no-unknown-property': 'error',
            'react/prop-types': 'off',
            'react-dom/no-dangerously-set-innerhtml': 'warn',
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
            // Prettier handles "quotes", "jsx-quotes", "indent", "semi", and "comma-dangle".
            'import/order': [
                'error',
                {
                    groups: [
                        'builtin',
                        'external',
                        'internal',
                        'parent',
                        'sibling',
                        'index',
                    ],
                    'newlines-between': 'always',
                    alphabetize: {
                        order: 'asc',
                        caseInsensitive: true,
                    },
                },
            ],
            complexity: [
                'warn',
                {
                    max: 15,
                },
            ],
        },
    },
    {
        files: ['scripts/**/*.mjs'],
        languageOptions: {
            globals: {
                ...globals.node,
            },
            ecmaVersion: 'latest',
            sourceType: 'module',
        },
        rules: {
            'no-console': 'off', // Scripts can use console
        },
    },
]
