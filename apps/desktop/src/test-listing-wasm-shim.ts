/**
 * What `virtual:cmdr-listing-wasm` resolves to under vitest (aliased in `vitest.config.ts`): the
 * demo's WASM listing engine exists only after `pnpm build:listing-wasm`, and only demo builds
 * alias it. A test of `src/lib/demo/listing.ts` replaces this with `vi.mock` and a fake engine;
 * anything that reaches it unmocked fails loudly.
 */
export default function init(): Promise<never> {
  return Promise.reject(new Error('The listing engine is not built under vitest: mock virtual:cmdr-listing-wasm'))
}

/** Never constructed: `init()` rejects first, so the demo's loader gives up before building an engine. */
export const DemoListing = { handles: (): boolean => false }
