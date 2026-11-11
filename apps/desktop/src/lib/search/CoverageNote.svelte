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
     * Each list is one line (`CoverageLine.svelte`): its folders sit in a tooltip, so a
     * search over `/` that met eighteen refused folders still leaves the results in view.
     *
     * Presentational: `SearchDialog.svelte` owns the state, the volume lookup, and the
     * IPC. Copy branches on the TYPED fields, never on message text.
     *
     * The wrapper stays mounted with `role="status"` even when there's nothing to say,
     * collapsing to zero height. A live region has to exist BEFORE its content changes
     * or screen readers miss the update, which is the same reason `QueryResults` keeps
     * its status bar mounted.
     */
    import Button from '#lib/ui/Button.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatNumber } from '#lib/file-explorer/selection/selection-info-utils.js'
    import CoverageLine from './CoverageLine.svelte'
    import { coverageLines, type CoverageLine as Line, type CoverageNote } from './coverage-note'

    interface Props {
        /** The last run's coverage gap, or `null` when it covered everything asked of it. */
        note: CoverageNote | null
        /** Display name of the drive the gap belongs to; `''` when it isn't mounted any more. */
        driveName: string
        /** Network drives get their own voice: Cmdr never pushes them toward indexing. */
        isNetwork: boolean
        /**
         * The drive's FIRST index is running right now, branch by branch. It changes
         * what an uncovered gap means: not "this drive isn't indexed" (which reads as
         * nothing happening), but "the walker hasn't got here yet". The offer to index
         * is withheld alongside it by `coverage-cta.svelte.ts`, since there is nothing
         * to turn on and pressing it would restart the phases.
         */
        isIndexing: boolean
        /**
         * Whether a drive index can serve the drive the gap belongs to. `false` for a
         * server over SFTP, WebDAV, or S3: an uncovered gap there reads as "search isn't
         * available here yet", and the "press Enter" line goes, since Enter doesn't
         * reach it either. `coverage-cta.svelte.ts` withholds the offer alongside.
         */
        canBeIndexed: boolean
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
        isIndexing,
        canBeIndexed,
        onIndexDrive,
        onSilenceDrive,
        onGrantFullDiskAccess = null,
    }: Props = $props()

    /** The drive's name, or a generic stand-in when it isn't in the live volume list. */
    const drive = $derived(driveName || tString('search.coverage.unnamedDrive'))

    /** An index-only answer on a drive Enter can walk: the gap costs one key to close. */
    const pressEnter = $derived(!!note && !note.live && canBeIndexed)

    const lines = $derived(
        note
            ? coverageLines(note, {
                  fullDiskAccess: onGrantFullDiskAccess !== null,
                  indexDrive: onIndexDrive !== null,
                  pressEnter,
              })
            : { unread: [], unsearched: [] },
    )

    /** Each list's sentence, counted. The folders themselves sit behind the line's icon. */
    function messageFor(line: Line): string {
        const count = line.paths.length
        const params = { count, countText: formatNumber(count) }
        switch (line.kind) {
            case 'failedRoots':
                return tString('search.coverage.line.failedRoots', params)
            case 'denied':
                return tString('search.coverage.line.denied', params)
            case 'offLimits':
                return tString('search.coverage.line.offLimits', params)
            case 'declined':
                return tString('search.coverage.line.declined', params)
            case 'stillCovering':
                return tString('search.coverage.line.stillCovering', params)
            case 'unresolved':
                return tString('search.coverage.line.unresolved', params)
            case 'uncovered':
                // First: a server is a network drive too, and the network voice ("unless
                // you ask") would promise an index nobody can ask for.
                if (!canBeIndexed) return tString('search.coverage.line.uncovered.unavailable', { ...params, drive })
                if (isNetwork) return tString('search.coverage.line.uncovered.network', { ...params, drive })
                if (isIndexing) return tString('search.coverage.line.uncovered.indexing', { ...params, drive })
                return tString('search.coverage.line.uncovered.local', { ...params, drive })
        }
    }

    /**
     * The footnote for ground the walk gave up on. Counted in PLACES when the backend
     * could group them, and count-free when it couldn't (this run's own walk gave up
     * on ground it recorded no path for). Both say Cmdr comes back to it: that is the
     * one fact separating this from the two lists above, and the reason it offers
     * nothing.
     */
    const abandonedMessage = $derived.by(() => {
        const locations = note?.live?.abandonedLocations ?? 0
        if (locations === 0) return tString('search.coverage.walk.abandoned')
        return tString('search.coverage.walk.abandonedCount', {
            count: locations,
            countText: formatNumber(locations),
        })
    })

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
        <!-- Ground nothing will read, in order of what it asks of you: the folder the walk
             set out to read (usually the scope, so the whole reason the list is empty),
             refused (grant access), closed for good, and declined on purpose. -->
        {#each lines.unread as line (line.kind)}
            <CoverageLine {line} message={messageFor(line)} />
            {#if line.kind === 'denied' && onGrantFullDiskAccess}
                <!-- The only line with a way out of its own, so its offer stays visible
                     outside the tooltip. -->
                <p class="message secondary">{tString('search.coverage.deniedFullDiskAccess')}</p>
                <div class="actions">
                    <Button variant="primary" size="mini" onclick={onGrantFullDiskAccess}>
                        {tString('search.coverage.setUpFullDiskAccess')}
                    </Button>
                </div>
            {/if}
        {/each}
        {#if note.live?.abandonedGround}
            <!-- The third way a run comes back short, and the quiet one: the walk
                 reached the end of its frontier having given up on folders along
                 the way. It sits UNDER the lists above rather than over them, as the
                 footnote to that family: it's true alongside a cancel or a
                 disconnect rather than instead of one, and unlike its neighbours it
                 asks nothing of the reader — Cmdr comes back to this ground on its
                 own. -->
            <p class="message secondary">{abandonedMessage}</p>
        {/if}
        {#each lines.unsearched as line (line.kind)}
            <CoverageLine {line} message={messageFor(line)} />
        {/each}
        {#if pressEnter}
            <!-- An index-only answer: this run was the DEBOUNCE's, which never walks
                 (Decision 7). So the gap it reports has a way out that costs one key,
                 except on a drive no index can serve, which Enter doesn't walk either. -->
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

    .actions {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-xs);
        margin-top: var(--spacing-xxs);
    }
</style>
