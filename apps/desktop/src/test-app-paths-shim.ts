/**
 * What `$app/paths` resolves to under vitest (aliased in `vitest.config.ts`), which runs the bare
 * Svelte plugin without SvelteKit's virtual `$app/*` modules. It mirrors SvelteKit's client
 * behavior for an app served at the root (the desktop app's base): `asset('icons/x.png')` is
 * `/icons/x.png`. Add an export here only when a component under test imports it.
 */
export function asset(file: string): string {
  return file.startsWith('/') ? file : `/${file}`
}
