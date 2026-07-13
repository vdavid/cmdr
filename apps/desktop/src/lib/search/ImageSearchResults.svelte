<script lang="ts">
    /**
     * "Text in images": the OCR search-results grid, rendered below the filename
     * results in the Search dialog (via QueryDialog's `resultsExtra` slot). A distinct
     * result type — a thumbnail grid, not a table row — so it lives in its own component
     * and owns its own data fetch + lifecycle rather than touching the shared
     * `results` / `cursorIndex` contract.
     *
     * Honesty (plan M1 § Coverage honesty): the section voices its own coverage from the
     * backend `mediaIndexVolumeState`, so an empty result is never a confident lie:
     *   - image indexing OFF        → say so, hint to turn it on.
     *   - a pass running for the vol → "still indexing images, results may be incomplete".
     *   - no images enriched yet     → "not indexed yet", distinct from a genuine miss.
     *   - enriched but no match      → an honest "no text found".
     *
     * Thumbnails reuse the EXISTING viewer preview scheme (`cmdr-media://` via
     * `mediaUrl`), never a media_index-produced thumbnail file (plan Decision 5). Each
     * displayed image mints a token; the component drops every token it minted when the
     * result set changes or it unmounts, so the backend token map never leaks.
     */
    import { onDestroy } from 'svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import {
        mediaIndexSearchOcr,
        mediaIndexVolumeState,
        mediaIndexThumbnailToken,
        mediaIndexDropThumbnailTokens,
        type MediaIndexVolumeState,
        type OcrHit,
    } from '$lib/tauri-commands'
    // The `cmdr-media://` URL is built ONLY via the viewer's `mediaUrl` (single source;
    // see `routes/viewer/CLAUDE.md`), so the grid reuses the exact preview origin.
    import { mediaUrl } from '../../routes/viewer/media-view'
    import { parseOcrSnippet } from './ocr-snippet'
    import { resolveMediaHitPath } from './media-path'

    interface Props {
        /** Live query text from the Search bar. */
        query: string
        /** Volume whose `media.db` to search (Search reads the local index → root). */
        volumeId: string
        /** Whether the Search dialog is showing; gates all work so a closed dialog is idle. */
        active: boolean
        /** Open the image (go to the file in the active pane). Receives the reconstructed
         *  absolute OS path (mount root prepended for a network volume). */
        onOpen: (path: string) => void
        /**
         * The volume's mount root, used to turn an index-relative OCR hit path into an
         * openable OS path (`/` for the local root, `/Volumes/<share>` for an SMB volume).
         * Defaults to `/`, so a local caller needs to pass nothing.
         */
        mountRoot?: string
        /**
         * Whether `volumeId` is a network (SMB) volume. Switches the coverage-honesty copy
         * to the network voice (opt-in hint, "disconnected" when paused). Local volumes use
         * the master-toggle voice.
         */
        isNetwork?: boolean
    }

    const { query, volumeId, active, onOpen, mountRoot = '/', isNetwork = false }: Props = $props()

    // Cap displayed tiles so a broad match doesn't decode hundreds of full-res images at
    // once (the preview scheme serves originals; the browser downscales). The backend
    // already caps the hit list; this bounds what we render + tokenize.
    const MAX_TILES = 48
    const DEBOUNCE_MS = 300

    interface Tile {
        path: string
        name: string
        segments: ReturnType<typeof parseOcrSnippet>
        /** `cmdr-media://` URL, or null when the image couldn't be tokenized (icon fallback). */
        thumbUrl: string | null
    }

    let volumeState = $state<MediaIndexVolumeState | null>(null)
    let tiles = $state<Tile[]>([])
    let totalHits = $state(0)
    let loading = $state(false)

    // Tokens this component minted for the currently-shown tiles, so we can drop exactly
    // them when the result set changes or the component unmounts.
    let mintedTokens: string[] = []
    // Monotonic request id: a late async response for a superseded query is discarded.
    let requestSeq = 0
    let debounceTimer: ReturnType<typeof setTimeout> | undefined

    function fileName(path: string): string {
        const parts = path.split('/')
        return parts[parts.length - 1] || path
    }

    async function releaseTokens(): Promise<void> {
        if (mintedTokens.length === 0) return
        const toDrop = mintedTokens
        mintedTokens = []
        await mediaIndexDropThumbnailTokens(toDrop).catch(() => {
            // Best-effort: a failed drop only risks a stale map entry, never correctness.
        })
    }

    function clearResults(): void {
        void releaseTokens()
        tiles = []
        totalHits = 0
    }

    async function runOcrSearch(seq: number): Promise<void> {
        loading = true
        try {
            // Volume state first, so the coverage-honesty copy is right even for zero hits.
            const [state, hits] = await Promise.all([
                mediaIndexVolumeState(volumeId),
                mediaIndexSearchOcr(volumeId, query, null),
            ])
            if (seq !== requestSeq) return
            volumeState = state
            totalHits = hits.length
            await buildTiles(hits.slice(0, MAX_TILES), seq)
        } catch {
            if (seq !== requestSeq) return
            // A failed search reads as "no results" honestly; the volume-state line still
            // renders if we have a prior snapshot. Never surface a raw error here.
            clearResults()
        } finally {
            if (seq === requestSeq) loading = false
        }
    }

    async function buildTiles(hits: OcrHit[], seq: number): Promise<void> {
        // Drop the previous set's tokens before minting the new one.
        await releaseTokens()
        const minted: string[] = []
        const built = await Promise.all(
            hits.map(async (hit): Promise<Tile> => {
                // Stored hit paths are index-relative; reconstruct the openable OS path so
                // both the thumbnail token (byte read) and the open action hit the real file
                // (a no-op passthrough for the local root, where the mount root is `/`).
                const osPath = resolveMediaHitPath(mountRoot, hit.path)
                let thumbUrl: string | null = null
                try {
                    const token = await mediaIndexThumbnailToken(osPath)
                    if (token !== null) {
                        minted.push(token)
                        thumbUrl = mediaUrl(token)
                    }
                } catch {
                    // No token → the tile falls back to a file glyph.
                }
                return {
                    path: osPath,
                    name: fileName(osPath),
                    segments: parseOcrSnippet(hit.snippet),
                    thumbUrl,
                }
            }),
        )
        if (seq !== requestSeq) {
            // Superseded mid-mint: drop what we just minted so it can't leak.
            void mediaIndexDropThumbnailTokens(minted).catch(() => {})
            return
        }
        mintedTokens = minted
        tiles = built
    }

    // Debounced fetch on query / visibility change. An empty query or a hidden dialog
    // clears the surface and does no work.
    $effect(() => {
        const trimmed = query.trim()
        const isActive = active
        if (debounceTimer) clearTimeout(debounceTimer)
        if (!isActive || trimmed === '') {
            requestSeq += 1
            clearResults()
            loading = false
            return
        }
        const seq = ++requestSeq
        debounceTimer = setTimeout(() => {
            void runOcrSearch(seq)
        }, DEBOUNCE_MS)
    })

    onDestroy(() => {
        if (debounceTimer) clearTimeout(debounceTimer)
        requestSeq += 1
        void releaseTokens()
    })

    // ── Derived coverage-honesty state ────────────────────────────────────────
    const showSection = $derived(active && query.trim() !== '')
    const enabled = $derived(volumeState?.enabled ?? false)
    const indexing = $derived(volumeState?.indexing ?? false)
    const enrichedCount = $derived(volumeState?.enrichedCount ?? 0)
    // Network-only honesty (M1.5): a network volume must be opted in, and it pauses when the
    // drive disconnects mid-pass (its already-indexed rows survive, so we still show them).
    const paused = $derived(volumeState?.paused ?? false)
    const networkOptIn = $derived(volumeState?.networkOptIn ?? false)
    const networkNeedsOptIn = $derived(isNetwork && enabled && !networkOptIn && volumeState !== null)
    const hasHits = $derived(tiles.length > 0)
    const moreCount = $derived(Math.max(0, totalHits - tiles.length))
</script>

{#if showSection}
    <section class="image-results" aria-label={tString('search.imageResults.title')}>
        <header class="ir-header">
            <span class="ir-title">{tString('search.imageResults.title')}</span>
            {#if hasHits}
                <span class="ir-count">
                    {moreCount > 0
                        ? tString('search.imageResults.countCapped', {
                              shownText: formatInteger(tiles.length),
                              totalText: formatInteger(totalHits),
                          })
                        : tString('search.imageResults.count', { totalText: formatInteger(totalHits) })}
                </span>
            {/if}
        </header>

        {#if !enabled && volumeState !== null}
            <p class="ir-notice">{tString('search.imageResults.off')}</p>
        {:else if networkNeedsOptIn}
            <p class="ir-notice">{tString('search.imageResults.networkOff')}</p>
        {:else}
            {#if paused}
                <p class="ir-notice">{tString('search.imageResults.paused')}</p>
            {:else if indexing}
                <p class="ir-notice ir-notice-indexing">
                    <Spinner size="sm" />
                    <span>{tString('search.imageResults.indexing')}</span>
                </p>
            {/if}

            {#if hasHits}
                <ul class="ir-grid" role="list">
                    {#each tiles as tile (tile.path)}
                        <li class="ir-tile-wrap">
                            <button
                                type="button"
                                class="ir-tile"
                                onclick={() => {
                                    onOpen(tile.path)
                                }}
                            >
                                <span class="ir-thumb">
                                    {#if tile.thumbUrl}
                                        <img src={tile.thumbUrl} alt={tile.name} loading="lazy" draggable="false" />
                                    {:else}
                                        <span class="ir-thumb-fallback">
                                            <Icon name="file" size={24} aria-hidden="true" />
                                        </span>
                                    {/if}
                                </span>
                                <span
                                    class="ir-name"
                                    use:useShortenMiddle={{
                                        text: tile.name,
                                        preferBreakAt: '.',
                                        startRatio: 0.7,
                                        tooltipWhenTruncated: true,
                                    }}
                                ></span>
                                <span class="ir-snippet">
                                    {#each tile.segments as seg (seg)}
                                        {#if seg.matched}<mark>{seg.text}</mark>{:else}{seg.text}{/if}
                                    {/each}
                                </span>
                            </button>
                        </li>
                    {/each}
                </ul>
            {:else if loading}
                <div class="ir-state"><Spinner size="sm" /></div>
            {:else if enabled && !indexing && enrichedCount === 0}
                <p class="ir-notice">{tString('search.imageResults.notIndexed')}</p>
            {:else if enabled && !indexing}
                <p class="ir-empty">{tString('search.imageResults.empty')}</p>
            {/if}
        {/if}
    </section>
{/if}

<style>
    .image-results {
        border-top: 1px solid var(--color-border-subtle);
        padding: var(--spacing-sm) var(--spacing-lg) var(--spacing-md);
        max-height: 40vh;
        overflow-y: auto;
    }

    .ir-header {
        display: flex;
        align-items: baseline;
        justify-content: space-between;
        gap: var(--spacing-sm);
        padding-bottom: var(--spacing-xs);
    }

    .ir-title {
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-secondary);
    }

    .ir-count {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    .ir-notice,
    .ir-empty {
        margin: var(--spacing-xs) 0;
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.4;
    }

    .ir-notice-indexing {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        color: var(--color-text-tertiary);
    }

    .ir-state {
        display: flex;
        justify-content: center;
        padding: var(--spacing-md);
    }

    .ir-grid {
        list-style: none;
        margin: var(--spacing-xs) 0 0;
        padding: 0;
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
        gap: var(--spacing-md);
    }

    .ir-tile-wrap {
        min-width: 0;
    }

    .ir-tile {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        width: 100%;
        padding: var(--spacing-xs);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
        background: var(--color-bg-secondary);
        cursor: default;
        text-align: left;
        min-width: 0;
    }

    .ir-tile:hover {
        background: var(--color-accent-subtle);
        border-color: var(--color-accent);
    }

    .ir-tile:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
    }

    .ir-thumb {
        display: flex;
        align-items: center;
        justify-content: center;
        aspect-ratio: 4 / 3;
        overflow: hidden;
        border-radius: var(--radius-sm);
        background: var(--color-bg-tertiary);
    }

    .ir-thumb img {
        width: 100%;
        height: 100%;
        object-fit: cover;
    }

    .ir-thumb-fallback {
        color: var(--color-text-tertiary);
    }

    .ir-name {
        font-size: var(--font-size-sm);
        font-weight: 500;
        color: var(--color-text-primary);
        overflow: hidden;
        white-space: nowrap;
        min-width: 0;
    }

    .ir-snippet {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        line-height: 1.35;
        display: -webkit-box;
        -webkit-line-clamp: 2;
        line-clamp: 2;
        -webkit-box-orient: vertical;
        overflow: hidden;
        overflow-wrap: anywhere;
    }

    .ir-snippet mark {
        background: var(--color-accent-subtle);
        color: var(--color-text-primary);
        border-radius: var(--radius-sm);
        padding: 0 var(--spacing-xxs);
    }
</style>
