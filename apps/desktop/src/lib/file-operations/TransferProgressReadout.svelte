<!--
    The dual-bar progress readout: a size bar and a count bar, each with its
    done/total, percent, and rate, plus one time-left cell shared by both.

    ONE widget serves both surfaces that show a running write operation — the
    copy/move/delete progress dialog and the Transfers window's rows — so the two
    can't drift apart in what they show or how it reads. The numbers themselves
    are single-sourced elsewhere (`progress-readout.ts` for speed and ETA,
    `$lib/units` for the text); this file owns the layout only.

    Every readout cell is a FIXED-width column sized for its own worst case, so
    the bars' width follows the window and nothing shifts as digits come and go
    ("9.99 GB" → "10.00 GB", "99%" → "100%"). The time cell right-aligns for the
    same reason: the phrase switches between "1h 8m left" and "56m 24s left" as
    an estimate firms up, and only its left edge may move.
-->
<script lang="ts">
    import ProgressBar from '$lib/ui/ProgressBar.svelte'
    import Size from '$lib/ui/Size.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { calculatePercentage, formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
    import { formatDuration, formatFilesPerSecond, seconds, type Seconds } from '$lib/units'
    import type { StallNotice } from './transfer/transfer-stall'

    interface Props {
        bytesDone: number
        /** Zero (unknown total) hides the size row; the count row always shows. */
        bytesTotal: number
        filesDone: number
        filesTotal: number
        /** The backend's rate. `null` during its warm-up, and while paused. */
        bytesPerSecond?: number | null
        /** The backend's rate. `null` during its warm-up, and while paused. */
        filesPerSecond?: number | null
        /** The SMOOTHED ETA (`createEtaSmoother`), never the raw backend value. */
        etaSeconds?: Seconds | null
        /** The backend's stall verdict; it displaces the countdown while set. */
        stall?: StallNotice | null
        /** The count row's noun: trash moves items, everything else files. */
        countKind?: 'files' | 'items'
        /** `compact` is a Transfers-window row, `comfortable` the dialog. */
        density?: 'comfortable' | 'compact'
    }

    const {
        bytesDone,
        bytesTotal,
        filesDone,
        filesTotal,
        bytesPerSecond = null,
        filesPerSecond = null,
        etaSeconds = null,
        stall = null,
        countKind = 'files',
        density = 'comfortable',
    }: Props = $props()

    const isCompact = $derived(density === 'compact')

    const hasSizeRow = $derived(bytesTotal > 0)
    const bytePercent = $derived(calculatePercentage(bytesDone, bytesTotal))
    const filePercent = $derived(calculatePercentage(filesDone, filesTotal))

    /** A zero rate is the estimator saying nothing has moved yet, not a speed. */
    const byteRate = $derived(bytesPerSecond !== null && bytesPerSecond > 0 ? bytesPerSecond : null)
    /** `null` for a rate that rounds to zero, so the cell stays empty. */
    const fileRateText = $derived(filesPerSecond === null ? null : formatFilesPerSecond(filesPerSecond))

    const countLabel = $derived(
        countKind === 'items'
            ? tString('fileOperations.transferProgress.progressItems')
            : tString('fileOperations.transferProgress.progressFiles'),
    )

    /** A countdown we no longer believe is worse than no countdown, so a stalled
     *  transfer says how long it's been still instead. */
    const timeText = $derived.by(() => {
        if (stall) {
            return tString('fileOperations.transferProgress.stallNotice', {
                duration: formatDuration(seconds(stall.stillForSeconds)),
            })
        }
        if (etaSeconds === null) return null
        return tString('fileOperations.transferProgress.etaRemaining', { duration: formatDuration(etaSeconds) })
    })
</script>

<div class="progress-readout" class:compact={isCompact}>
    <div class="bars">
        {#if hasSizeRow}
            {#if !isCompact}
                <span class="bar-label">{tString('fileOperations.transferProgress.progressSize')}</span>
            {/if}
            <ProgressBar
                value={bytesDone / bytesTotal}
                size={isCompact ? 'sm' : 'md'}
                ariaLabel={tString('fileOperations.transferProgress.sizeProgressAria')}
            />
            <span class="amount"><Size bytes={bytesDone} /> / <Size bytes={bytesTotal} /></span>
            <span class="percent">{formatNumber(bytePercent)}%</span>
            <span class="rate">
                {#if byteRate !== null}<Trans
                        key="fileOperations.shared.byteRate"
                        snippets={{ size: byteRateSize }}
                    />{/if}
            </span>
        {/if}

        {#if !isCompact}
            <span class="bar-label">{countLabel}</span>
        {/if}
        <ProgressBar
            value={filesTotal > 0 ? filesDone / filesTotal : 0}
            size={isCompact ? 'sm' : 'md'}
            ariaLabel={tString('fileOperations.transferProgress.fileProgressAria')}
        />
        <span class="amount">{formatNumber(filesDone)} / {formatNumber(filesTotal)}</span>
        <span class="percent">{formatNumber(filePercent)}%</span>
        <span class="rate">{fileRateText ?? ''}</span>
    </div>

    <!-- Rendered even while empty: the cell holds its width so the bars don't
         resize the moment the estimator warms up and the text appears. -->
    <span class="time" class:stalled={stall !== null}>{timeText ?? ''}</span>
</div>

{#snippet byteRateSize(children: import('svelte').Snippet)}<Size bytes={byteRate ?? 0} />{@render children()}{/snippet}

<style>
    .progress-readout {
        /* Column widths in `ch` (the digit width of the current font), each
           sized for the widest string that column can hold: "999.99 GB /
           999.99 GB", "100%", "999.99 MB/s", "59m 59s left". `minmax` rather
           than a hard width so a rarer outlier (a terabyte pair, a stall
           notice) grows instead of clipping. */
        --spacing-readout-amount: 20ch;
        --spacing-readout-percent: 5.5ch;
        --spacing-readout-rate: 11ch;
        --spacing-readout-time: 12ch;
        /* Below this the bar reads as a smudge rather than progress, so it's
           the window's min width that gives way, not the bar. */
        --spacing-readout-bar-min: 80px;

        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        font-size: var(--font-size-sm);
        font-variant-numeric: tabular-nums;
    }

    .bars {
        flex: 1;
        min-width: 0;
        display: grid;
        grid-template-columns:
            auto
            minmax(var(--spacing-readout-bar-min), 1fr)
            minmax(var(--spacing-readout-amount), auto)
            minmax(var(--spacing-readout-percent), auto)
            minmax(var(--spacing-readout-rate), auto);
        align-items: center;
        gap: var(--spacing-xs) var(--spacing-sm);
    }

    /* A list row drops the row labels: the units in the amounts already say
       which bar is which, and the space buys back bar width. */
    .compact .bars {
        grid-template-columns:
            minmax(var(--spacing-readout-bar-min), 1fr)
            minmax(var(--spacing-readout-amount), auto)
            minmax(var(--spacing-readout-percent), auto)
            minmax(var(--spacing-readout-rate), auto);
        gap: var(--spacing-xs);
    }

    .compact {
        --spacing-readout-bar-min: 64px;

        gap: var(--spacing-xs);
    }

    .bar-label {
        color: var(--color-text-tertiary);
    }

    .amount,
    .percent,
    .rate,
    .time {
        white-space: nowrap;
        text-align: right;
    }

    .amount,
    .percent {
        color: var(--color-text-secondary);
    }

    .rate {
        color: var(--color-text-secondary);
    }

    .time {
        min-width: var(--spacing-readout-time);
        color: var(--color-text-tertiary);
    }

    /* A stalled readout is the one thing on the row worth noticing. */
    .time.stalled {
        color: var(--color-text-secondary);
    }
</style>
