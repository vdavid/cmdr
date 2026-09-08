/**
 * Helpers for expressing Svelte 5 reactive dependencies that the surrounding code doesn't otherwise
 * read. See `DETAILS.md` § "Declaring a reactive dependency".
 */

/**
 * Registers its arguments as dependencies of the enclosing `$effect` / `$derived`, for the case where
 * an effect must re-run on a value it never actually uses in its body (a version counter, a setting
 * whose change invalidates a cached answer, a prop that moves the window a comparison is made
 * against).
 *
 * Arguments are evaluated at the call site, synchronously inside the effect, which is what registers
 * the dependency. The body is empty on purpose: the read IS the work.
 *
 * ```ts
 * $effect(() => {
 *   dependOn(provider, model) // a provider or model change moves the window this compares against
 *   recomputeTokenWindow()
 * })
 * ```
 *
 * ❌ Don't write `void provider` for this. `@typescript-eslint/no-meaningless-void-operator` rejects
 * `void` on a non-call expression, and its autofix leaves a bare expression statement that
 * `no-unused-expressions` rejects in turn, so the two rules together have no legal spelling of the
 * idiom. `void somePromise()` is unaffected and stays the way to discard a floating promise.
 */
export function dependOn(..._values: unknown[]): void {
  // Intentionally empty. See the doc comment: evaluating the arguments is the entire point.
}
