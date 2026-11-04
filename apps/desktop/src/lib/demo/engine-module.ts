/**
 * The WASM engine module (`crates/cmdr-listing-wasm`), loaded once per page: the listing engine
 * (`listing.ts`) and the menu bar (`menu-engine.ts`) both live in it, so both wait on this one
 * fetch. Resolves with `null` when the module didn't load, after one loud console line.
 */
type EngineModule = typeof import('virtual:cmdr-listing-wasm')

let loading: Promise<EngineModule | null> | undefined

export function loadEngineModule(): Promise<EngineModule | null> {
  loading ??= (async () => {
    const wasm = await import('virtual:cmdr-listing-wasm')
    await wasm.default()
    return wasm
  })().catch((error: unknown) => {
    // Once, loudly: without the engine every listing command shows up as unanswered.
    // eslint-disable-next-line no-console -- the demo's developer reads the devtools console
    console.error('[demo] the listing engine did not load', error)
    return null
  })
  return loading
}

/** Forgets the loaded module, so a test can load a fresh fake. */
export function _resetEngineModuleForTests(): void {
  loading = undefined
}
