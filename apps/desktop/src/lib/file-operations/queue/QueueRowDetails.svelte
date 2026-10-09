<!--
    The panel under an EXPANDED queue row: every top-level source as a full
    path, where the operation writes, the file it's on right now, and when it
    started. The answer to "what exactly is this doing?", which the one-line row
    has no room for.

    The paths and times come from `get_operation_details`, fetched when the
    panel opens and again whenever the row's lifecycle status changes (a queued
    operation that starts gains a start time). ❌ Never from `operations-changed`,
    which stays thin, and ❌ no counts or rates here: the readout above already
    shows those, from the row's session. DETAILS § "Row details".
-->
<script lang="ts">
    import { onDestroy } from 'svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { formatDateTime } from '$lib/settings/reactive-settings.svelte'
    import { formatDuration, seconds } from '$lib/units'
    import type { OperationSnapshot, WriteProgressEvent } from '$lib/ipc/bindings'
    import { createOperationDetailsLoader } from './operation-details.svelte'

    interface Props {
        snapshot: OperationSnapshot
        progress: WriteProgressEvent | null
        /** The panel's element id, which the row's toggle names in `aria-controls`. */
        id: string
    }

    const { snapshot, progress, id }: Props = $props()

    const loader = createOperationDetailsLoader()
    onDestroy(() => {
        loader.dispose()
    })

    // Primitives, so a snapshot rebuild that changed nothing about THIS row
    // (another operation started or finished) doesn't ask the backend again.
    const operationId = $derived(snapshot.operationId)
    const status = $derived(snapshot.status)

    /** What to ask about, re-derived only when the id or the status really
     *  changes: queued → running stamps a start time, so that's worth a
     *  second question. */
    const request = $derived({ operationId, status })
    $effect(() => {
        loader.load(request.operationId)
    })

    const details = $derived(loader.details)
    const hiddenSourceCount = $derived(details === null ? 0 : details.sourceCount - details.sourcePaths.length)
    const isLive = $derived(status === 'running' || status === 'paused')

    /** The file in flight, off the live tick the row already renders. Only a
     *  live row has one: a queued operation hasn't begun, a failed one is over. */
    const currentFile = $derived(isLive ? (progress?.currentFile ?? null) : null)

    /** A wall clock for the elapsed line, ticking only while the operation is
     *  live and this panel is open. */
    let nowSeconds = $state(Math.floor(Date.now() / 1000))
    $effect(() => {
        if (!isLive) return
        nowSeconds = Math.floor(Date.now() / 1000)
        const timer = setInterval(() => {
            nowSeconds = Math.floor(Date.now() / 1000)
        }, 1000)
        return () => {
            clearInterval(timer)
        }
    })

    /** 0 is the backend's "no clock reading", never a date in 1970. */
    const startedAt = $derived(details?.startedAt ? details.startedAt : null)
    const queuedAt = $derived(status === 'queued' && details?.queuedAt ? details.queuedAt : null)
    const elapsed = $derived(
        isLive && startedAt !== null ? formatDuration(seconds(Math.max(0, nowSeconds - startedAt))) : null,
    )
</script>

<div class="details" {id}>
    {#if details === null && loader.unavailable}
        <p class="note">{tString('queue.details.unavailable')}</p>
    {/if}
    <dl class="facts">
        {#if details !== null && details.sourcePaths.length > 0}
            <dt>{tString('queue.details.from')}</dt>
            <dd>
                <!-- A long selection scrolls inside the panel, so the list has to
                     be reachable by keyboard as well as by wheel. -->
                <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
                <div class="sources" role="region" aria-label={tString('queue.details.sourcesAria')} tabindex="0">
                    <ul>
                        {#each details.sourcePaths as path, index (index)}
                            <li class="path selectable">{path}</li>
                        {/each}
                        {#if hiddenSourceCount > 0}
                            <li class="more">
                                {tString('queue.details.moreSources', {
                                    count: hiddenSourceCount,
                                    countText: formatInteger(hiddenSourceCount),
                                })}
                            </li>
                        {/if}
                    </ul>
                </div>
            </dd>
        {/if}
        {#if details?.destinationPath}
            <dt>{tString('queue.details.to')}</dt>
            <dd class="path selectable">{details.destinationPath}</dd>
        {/if}
        {#if currentFile}
            <dt>{tString('queue.details.currentFile')}</dt>
            <dd class="path selectable">{currentFile}</dd>
        {/if}
        {#if queuedAt !== null}
            <dt>{tString('queue.details.waitingSince')}</dt>
            <dd>{formatDateTime(queuedAt)}</dd>
        {/if}
        {#if startedAt !== null}
            <dt>{tString('queue.details.started')}</dt>
            <dd>{formatDateTime(startedAt)}</dd>
        {/if}
        {#if elapsed !== null}
            <dt>{tString('queue.details.elapsed')}</dt>
            <dd class="tabular">{elapsed}</dd>
        {/if}
    </dl>
</div>

<style>
    .details {
        grid-column: 3 / -1;
        min-width: 0;
        padding: var(--spacing-xs) 0 var(--spacing-xxs);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
    }

    .note {
        margin: 0;
        color: var(--color-text-tertiary);
    }

    .facts {
        display: grid;
        grid-template-columns: auto minmax(0, 1fr);
        gap: var(--spacing-xxs) var(--spacing-sm);
        margin: 0;
    }

    dt {
        color: var(--color-text-tertiary);
        white-space: nowrap;
    }

    dd {
        margin: 0;
        min-width: 0;
    }

    /* A big selection scrolls in place instead of pushing every row below it
       off the window. */
    .sources {
        max-height: 8.5em;
        overflow-y: auto;
    }

    .sources ul {
        margin: 0;
        padding: 0;
        list-style: none;
    }

    .path {
        font-family: var(--font-mono);
        /* A long path wraps instead of widening the row. `anywhere` rather than
           `break-word` so a break can land mid-segment too: one 200-character
           folder name carries no break opportunity of its own. */
        overflow-wrap: anywhere;
    }

    .more {
        color: var(--color-text-tertiary);
    }

    .tabular {
        font-variant-numeric: tabular-nums;
    }

    /* Paths are the one thing here worth copying out. */
    .selectable {
        user-select: text;
        -webkit-user-select: text;
        cursor: text;
    }
</style>
