import { RuleTester } from 'eslint'
import tseslint from 'typescript-eslint'
import svelteParser from 'svelte-eslint-parser'
import rule from './no-confusable-callback-params.js'

// Flat-config RuleTester (ESLint 9+) on the typescript-eslint parser: the rule
// visits `TSFunctionType` nodes, which espree can't even produce. RuleTester
// auto-detects Vitest's `describe`/`it` globals, so `run` is called at the top
// level (it can't be nested inside our own `it`).
const ruleTester = new RuleTester({
  languageOptions: { parser: tseslint.parser, ecmaVersion: 'latest', sourceType: 'module' },
})

ruleTester.run('no-confusable-callback-params', rule, {
  valid: [
    // Fewer than two non-rest parameters: nothing to confuse.
    { code: `type F = () => void` },
    { code: `type F = (a: string) => void` },
    // Distinct, unrelated types.
    { code: `type F = (id: string, count: number) => void` },
    // A rest parameter never counts toward the pair, however many others there are.
    { code: `type F = (a: string, ...rest: string[]) => void` },
    // Two rest-only-ish params where only one is typed and the other is a rest:
    // still under the two-non-rest-parameter floor.
    { code: `type F = (a: string, ...rest: number[]) => void` },
    // Untyped parameters can't be judged, so they're skipped rather than treated
    // as wildcards; with only one parameter left to compare, there's nothing to flag.
    { code: `type F = (a: string, b) => void` },
    { code: `type F = (a, b) => void` },
    // A generic used where NO type parameter is in scope is just an unresolved
    // (or externally-bound) type name, not a confusable callback parameter.
    { code: `type F = (a: K, b: string) => void` },
    // Destructured object-payload params (the intended fix shape) aren't flagged:
    // there's exactly one non-rest parameter, whatever its own type looks like.
    { code: `type F = (args: { volumeId: string; volumePath: string }) => void` },
  ],
  invalid: [
    // Two parameters, identical primitive type.
    {
      code: `type F = (a: string, b: string) => void`,
      errors: [{ messageId: 'confusableParams' }],
    },
    // Three parameters, all the same type: the ⌘A-style incident shape.
    {
      code: `type F = (volumeId: string, volumePath: string, targetPath: string) => void`,
      errors: [{ messageId: 'confusableParams' }],
    },
    // Optional vs. required doesn't change the type text: `number` still duplicates `number`.
    {
      code: `type F = (x: number, y?: number) => void`,
      errors: [{ messageId: 'confusableParams' }],
    },
    // A duplicated union type, compared after stripping whitespace.
    {
      code: `type F = (a: Foo | null, b: Foo   |   null) => void`,
      errors: [{ messageId: 'confusableParams' }],
    },
    // THE original incident: two DIFFERENT type texts (`K` vs `SettingsValues[K]`),
    // so clause 1 alone would miss it — `K` is a bare reference to a type parameter
    // declared on the enclosing function.
    {
      code: `function getSetting<K extends string>(listener: (id: K, value: SettingsValues[K]) => void) {}`,
      errors: [{ messageId: 'confusableParams' }],
    },
    // Same shape, but the type parameter is declared on the function TYPE itself
    // (not an enclosing declaration), and reused across both parameters.
    {
      code: `type F = <K>(id: K, value: K) => void`,
      errors: [{ messageId: 'confusableParams' }],
    },
    // Two DIFFERENT type parameters are still each individually confusable: an
    // unconstrained generic could be instantiated as anything, including
    // whatever its neighbour resolves to — the rule doesn't require a shared
    // identity, just that BOTH sides are bare generics in scope.
    {
      code: `type F = <K, V>(key: K, value: V) => void`,
      errors: [{ messageId: 'confusableParams' }],
    },
    // A bare generic paired with an unrelated concrete type still trips clause 2:
    // `id: K` could be instantiated as `number`, matching its sibling.
    {
      code: `function on<K>(cb: (id: K, count: number) => void) {}`,
      errors: [{ messageId: 'confusableParams' }],
    },
    // Four parameters, two independent duplicate pairs.
    {
      code: `type F = (a: string, b: string, c: number, d: number) => void`,
      errors: [{ messageId: 'confusableParams' }],
    },
    // The rest parameter is excluded from the comparison, but the two typed
    // parameters ahead of it are still checked.
    {
      code: `type F = (a: string, b: string, ...rest: number[]) => void`,
      errors: [{ messageId: 'confusableParams' }],
    },
  ],
})

// The rule fires identically inside a `.svelte` script block: `TSFunctionType`
// nodes come from the same typescript-eslint grammar embedded via
// `svelte-eslint-parser`, with no type information required either way.
const svelteRuleTester = new RuleTester({
  languageOptions: {
    parser: svelteParser,
    parserOptions: { parser: tseslint.parser },
    ecmaVersion: 'latest',
    sourceType: 'module',
  },
})

svelteRuleTester.run('no-confusable-callback-params (svelte)', rule, {
  valid: [
    {
      code: `<script lang="ts">\n  let onSelect: (id: string) => void\n</script>`,
      filename: 'Component.svelte',
    },
  ],
  invalid: [
    {
      code: `<script lang="ts">\n  let listener: (volumeId: string, volumePath: string, targetPath: string) => void\n</script>`,
      filename: 'Component.svelte',
      errors: [{ messageId: 'confusableParams' }],
    },
  ],
})
