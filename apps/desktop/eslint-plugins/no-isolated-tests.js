/**
 * ESLint rule to detect test files that don't import any source code.
 *
 * Tests that only import types or test utilities (but no actual source code)
 * are likely testing their own mocks rather than real application behavior.
 * This catches patterns like defining local mock functions and testing those
 * instead of importing and testing the actual implementation.
 */

/** @type {import('eslint').Rule.RuleModule} */
export default {
    meta: {
        type: 'problem',
        docs: {
            description: 'Test files must import actual source code to test',
            recommended: true,
        },
        messages: {
            noSourceImports:
                'Test file has no source code imports. Tests should import and exercise actual application code, not just test local mocks.',
        },
        schema: [],
    },
    create(context) {
        const filename = context.filename || context.getFilename()

        // Only apply to test files
        if (!filename.match(/\.test\.[tj]sx?$/)) {
            return {}
        }

        let hasSourceImport = false

        return {
            ImportDeclaration(node) {
                const source = node.source.value

                // Check if this is a source import (not node_modules, not vitest, etc.)
                const isSourceImport =
                    source.startsWith('$lib/') ||
                    source.startsWith('./') ||
                    source.startsWith('../')

                // Skip if it's a type-only import
                const isTypeOnlyImport = node.importKind === 'type'

                // Check if all specifiers are type imports
                const allSpecifiersAreTypes =
                    node.specifiers.length > 0 &&
                    node.specifiers.every((spec) => spec.importKind === 'type')

                if (isSourceImport && !isTypeOnlyImport && !allSpecifiersAreTypes) {
                    hasSourceImport = true
                }
            },
            'Program:exit'(node) {
                if (!hasSourceImport) {
                    context.report({
                        node,
                        messageId: 'noSourceImports',
                    })
                }
            },
        }
    },
}
