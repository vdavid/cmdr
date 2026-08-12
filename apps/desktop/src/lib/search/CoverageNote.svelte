<script lang="ts">
    /**
     * The honest answer to "why did this search come back empty?".
     *
     * A search covers one volume, and that volume may have no index (`uncoveredScopes`)
     * or an index that doesn't hold the folder asked for (`unresolvedScopes`). Both used
     * to render as a plain "No files match these criteria", which is a wrong answer with
     * a confident face. This strip says which scopes weren't covered, and offers the
     * per-drive indexing flow when there's a drive to act on.
     *
     * Presentational: `SearchDialog.svelte` owns the state, the volume lookup, and the
     * IPC. Copy branches on the TYPED fields, never on message text.
     *
     * The wrapper stays mounted with `role="status"` even when there's nothing to say,
     * collapsing to zero height. A live region has to exist BEFORE its content changes
     * or screen readers miss the update, which is the same reason `QueryResults` keeps
     * its status bar mounted.
     */
    import Button from '$lib/ui/Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import type { CoverageNote } from './coverage-note'

    interface Props {
        /** The last run's coverage gap, or `null` when it covered everything asked of it. */
        note: CoverageNote | null
        /** Display name of the drive the gap belongs to; `''` when it isn't mounted any more. */
        driveName: string
        /** Network drives get their own voice: Cmdr never pushes them toward indexing. */
        isNetwork: boolean
        /**
         * Offers indexing for that drive. `null` hides the actions entirely: the user
         * silenced this drive, or there's no drive to act on (an unresolved path on a
         * drive that IS indexed).
         */
        onIndexDrive: (() => void) | null
        /** "Don't ask again" for this drive. Present exactly when `onIndexDrive` is. */
        onSilenceDrive: () => void
        /**
         * Routes into the Full Disk Access setup. `null` hides the offer, which is
         * every case where granting it would change nothing: no folder was actually
         * refused, Cmdr already has the permission, or this isn't macOS. The refusal
         * itself is still stated — a gap with no way out is still a gap
         * (`coverage-note.ts::offersFullDiskAccess`).
         */
        onGrantFullDiskAccess?: (() => void) | null
    }

    const {
        note,
        driveName,
        isNetwork,
        onIndexDrive,
        onSilenceDrive,
        onGrantFullDiskAccess = null,
    }: Props = $props()

    /** The drive's name, or a generic stand-in when it isn't in the live volume list. */
    const drive = $derived(driveName || tString('search.coverage.unnamedDrive'))

    const uncoveredMessage = $derived(
        isNetwork
            ? tString('search.coverage.uncovered.network', { drive })
            : tString('search.coverage.uncovered.local', { drive }),
    )

    /**
     * Why a live run's list is a lower bound, in the run's own words. `''` for a walk
     * that finished, which is the only ending that leaves nothing to explain (the
     * status bar has already said the list is short; this says which kind of short).
     */
    const walkMessage = $derived.by(() => {
        const walk = note?.live?.walk
        if (walk === 'cancelled') return tString('search.coverage.walk.cancelled')
        if (walk === 'interrupted') return tString('search.coverage.walk.interrupted', { drive })
        return ''
    })
</script>

<div class="coverage-note" class:is-empty={!note} role="status">
    {#if note}
        {#if walkMessage}
            <p class="message">{walkMessage}</p>
        {/if}
        {#if note.live?.abandonedGround}
            <!-- The third way a run comes back short, and the quiet one: the walk
                 reached the end of its frontier having given up on folders along
                 the way. Its own line, because it's true alongside a cancel or a
                 disconnect rather than instead of one. -->
            <p class="message">{tString('search.coverage.walk.abandoned')}</p>
        {/if}
        {#if note.live && note.live.permissionDenied.length > 0}
            <!-- A folder somebody refused us. Its own sentence, and the only one of
                 the two with a way out: on macOS that's Full Disk Access, offered
                 only when Cmdr doesn't already have it. -->
            <p class="message">
                {tString('search.coverage.denied', { count: note.live.permissionDenied.length })}
            </p>
            <ul class="scopes">
                {#each note.live.permissionDenied as path (path)}
                    <li>{path}</li>
                {/each}
            </ul>
            {#if onGrantFullDiskAccess}
                <p class="message secondary">{tString('search.coverage.deniedFullDiskAccess')}</p>
                <div class="actions">
                    <Button variant="primary" size="mini" onclick={onGrantFullDiskAccess}>
                        {tString('search.coverage.setUpFullDiskAccess')}
                    </Button>
                </div>
            {/if}
        {/if}
        {#if note.live && note.live.declined.length > 0}
            <!-- Ground Cmdr won't read on purpose. ❌ Never offer a permission here:
                 nothing the user can grant opens a snapshot tree. -->
            <p class="message">
                {tString('search.coverage.declined', { count: note.live.declined.length })}
            </p>
            <ul class="scopes">
                {#each note.live.declined as path (path)}
                    <li>{path}</li>
                {/each}
            </ul>
        {/if}
        {#if note.live && note.live.stillCovering.length > 0}
            <p class="message">
                {tString('search.coverage.stillCovering', { count: note.live.stillCovering.length })}
            </p>
            <ul class="scopes">
                {#each note.live.stillCovering as path (path)}
                    <li>{path}</li>
                {/each}
            </ul>
        {/if}
        {#if note.uncoveredScopes.length > 0}
            <p class="message">{uncoveredMessage}</p>
            <ul class="scopes">
                {#each note.uncoveredScopes as path (path)}
                    <li>{path}</li>
                {/each}
            </ul>
        {/if}
        {#if note.unresolvedScopes.length > 0}
            <p class="message">
                {tString('search.coverage.unresolved', { count: note.unresolvedScopes.length })}
            </p>
            <ul class="scopes">
                {#each note.unresolvedScopes as path (path)}
                    <li>{path}</li>
                {/each}
            </ul>
        {/if}
        {#if !note.live}
            <!-- An index-only answer: this run was the DEBOUNCE's, which never walks
                 (Decision 7). So the gap it reports has a way out that costs one key. -->
            <p class="message secondary">{tString('search.coverage.pressEnter')}</p>
        {/if}
        {#if onIndexDrive}
            <div class="actions">
                <Button variant="primary" size="mini" onclick={onIndexDrive}>
                    {tString('search.coverage.indexDrive')}
                </Button>
                <Button variant="secondary" size="mini" onclick={onSilenceDrive}>
                    {tString('search.coverage.dontAskAgain')}
                </Button>
            </div>
        {/if}
    {/if}
</div>

<style>
    .coverage-note {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        padding: var(--spacing-xs) var(--spacing-dialog);
        border-bottom: 1px solid var(--color-border-subtle);
        background: var(--color-bg-secondary);
        flex-shrink: 0;
    }

    /* Nothing to say: collapse completely rather than leaving a bordered empty strip.
       Still mounted, so the live region survives to announce the next run. */
    .coverage-note.is-empty {
        padding: 0;
        border-bottom: none;
        gap: 0;
    }

    .message {
        margin: 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    /* The follow-up under a refused folder: what would open it, and not the headline. */
    .message.secondary {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
    }

    .scopes {
        margin: 0;
        padding-left: var(--spacing-md);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        font-family: var(--font-mono);
        overflow-wrap: anywhere;
    }

    .actions {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-xs);
        margin-top: var(--spacing-xxs);
    }
</style>
