<script lang="ts">
    /**
     * PathPills: A path rendered as a strip of clickable ancestor pills.
     *
     * Replaces the flat `parentPath` string in `SearchResults` rows. Each segment is a
     * small button; clicking navigates the active pane to that ancestor folder AND closes
     * the dialog (the parent wires both via `onPick`).
     *
     * Per search-redesign-plan §3.8:
     *   - Pills are NOT in the keyboard Tab order (`tabindex="-1"`). Putting them in the
     *     Tab order would break the row's arrow-down keyboard flow inside the virtualized
     *     results list. The row's primary cell is the keyboard target; the dialog wires
     *     `⌥←` / `⌥→` on the cursor row's path as the keyboard equivalent.
     *   - macOS and Linux only: split strictly on `/`. No `\` handling (Windows is out of
     *     scope for the redesign).
     *   - Pill chrome: `--radius-sm`, `--spacing-xxs / --spacing-xs` padding, `--font-size-xs`,
     *     no border by default, hover background = `--color-bg-tertiary`.
     */

    interface Props {
        /** Path to render (typically `entry.parentPath`; may also be the entry's own path). */
        path: string
        /**
         * Called when the user clicks a pill. Receives the absolute path to that ancestor.
         * The parent is expected to navigate the active pane and close the dialog.
         */
        onPick: (path: string) => void
    }

    const { path, onPick }: Props = $props()

    /**
     * Splits a POSIX-style path into `{ label, fullPath }` segments. Returns one segment
     * per directory component, each `fullPath` carrying the absolute path up to and
     * including that segment. Empty input or a bare `/` returns a single "/" pill.
     *
     * Examples:
     *   `/Users/dave/code/proj`  → [{/, Users}, {/Users, dave}, ...]  (label "Users", path "/Users")
     *   `/`                     → [{label: "/", fullPath: "/"}]
     *   `relative/path`         → [{relative}, {relative/path}]      (no leading "/")
     */
    function splitPath(input: string): { label: string; fullPath: string }[] {
        if (!input) return []
        // Split on `/`, then drop empty parts so leading/trailing/duplicate slashes don't
        // produce empty pills. Keep one segment per non-empty part.
        const isAbsolute = input.startsWith('/')
        const parts = input.split('/').filter((p) => p.length > 0)
        if (parts.length === 0) {
            // Bare "/" or empty: render a single root pill so the column isn't empty.
            return isAbsolute ? [{ label: '/', fullPath: '/' }] : []
        }
        const out: { label: string; fullPath: string }[] = []
        let acc = isAbsolute ? '' : ''
        for (const part of parts) {
            acc = isAbsolute || out.length > 0 ? `${acc}/${part}` : part
            out.push({ label: part, fullPath: acc })
        }
        return out
    }

    const segments = $derived(splitPath(path))
</script>

{#if segments.length > 0}
    <span class="path-pills" aria-label={path}>
        {#each segments as seg, i (seg.fullPath)}
            {#if i > 0}
                <span class="sep" aria-hidden="true">/</span>
            {/if}
            <button
                type="button"
                class="pill"
                tabindex="-1"
                title={seg.fullPath}
                onclick={(e) => {
                    e.stopPropagation()
                    onPick(seg.fullPath)
                }}
            >
                {seg.label}
            </button>
        {/each}
    </span>
{/if}

<style>
    .path-pills {
        display: inline-flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--spacing-xxs);
        min-width: 0;
        overflow: hidden;
    }

    .sep {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        user-select: none;
    }

    .pill {
        background: transparent;
        border: 0;
        padding: var(--spacing-xxs) var(--spacing-xs);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-xs);
        font-family: inherit;
        color: var(--color-text-tertiary);
        line-height: 1.2;
        white-space: nowrap;
        transition:
            background var(--transition-base),
            color var(--transition-base);
    }

    .pill:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    /* Mouse focus ring: standard 2-layer accent ring (matches the rest of the app).
       Pills aren't in Tab order, so the keyboard branch never reaches this rule;
       click-driven focus still benefits from a visible ring. */
    .pill:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }
</style>
