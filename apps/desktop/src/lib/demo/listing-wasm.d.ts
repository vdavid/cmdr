/**
 * The WASM listing engine's JS glue (`crates/cmdr-listing-wasm/pkg/cmdr_listing_wasm.js`, built by
 * `pnpm build:listing-wasm`), which `vite.config.js` aliases in demo builds only. Declared here, by
 * hand, so the typecheck passes without the generated package: keep it to the subset `listing.ts`
 * calls, and in step with `crates/cmdr-listing-wasm/src/bindings.rs`.
 */
declare module 'virtual:cmdr-listing-wasm' {
  /** One entry of the tree, as `crates/cmdr-listing-wasm/src/tree.rs` reads it. */
  interface DemoTreeEntry {
    path: string
    kind: 'file' | 'dir' | 'symlink'
    size?: number
    modifiedMs?: number
    createdMs?: number
    symlinkTarget?: string
  }

  export class DemoListing {
    constructor(
      tree: { volumeId: string; root: string; entries: readonly DemoTreeEntry[] },
      // `target` is the window label of an event the app sends with `emit_to`.
      onEvent: (event: string, payload: unknown, target?: string) => void,
      // Real bytes for some files, by path.
      contents?: Readonly<Record<string, Uint8Array>>,
    )
    static commands(): string[]
    static handles(command: string): boolean
    static setLogLevel(level: 'error' | 'warn' | 'info' | 'debug' | 'trace'): void
    invoke(command: string, args: unknown): Promise<unknown>
    /** A viewer media token's bytes; `windowLabel` hears `viewer-media-load` for it. */
    mediaBody(token: string, windowLabel: string): { mime: string; bytes: Uint8Array } | undefined
    /** Frees the viewer sessions of the window `windowLabel`. */
    closeViewerWindow(windowLabel: string): void
  }

  /** Fetches and instantiates `cmdr_listing_wasm_bg.wasm` next to the glue. */
  export default function init(): Promise<unknown>
}
