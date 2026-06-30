<script lang="ts">
    import Size from '$lib/ui/Size.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import Trans from '$lib/intl/Trans.svelte'
    import { t, tString } from '$lib/intl/messages.svelte'

    interface Props {
        sourceFolderPath: string
        scanFilesFound: number
        scanDirsFound: number
        scanBytesFound: number
        scanFilesPerSec: number | null
        scanBytesPerSec: number | null
        scanCurrentDir: string | null
        currentFile: string | null
    }

    const {
        sourceFolderPath,
        scanFilesFound,
        scanDirsFound,
        scanBytesFound,
        scanFilesPerSec,
        scanBytesPerSec,
        scanCurrentDir,
        currentFile,
    }: Props = $props()
</script>

<!-- Source path -->
<div class="source-path">
    <span class="source-path-label">{tString('fileOperations.scanPhase.fromLabel')}</span>
    <span class="source-path-value" use:useShortenMiddle={{ text: sourceFolderPath, preferBreakAt: '/' }}></span>
</div>

<!-- Running tallies -->
<div class="scan-wait-stats">
    <div class="scan-stat">
        <span class="scan-value"><Size bytes={scanBytesFound} /></span>
    </div>
    <span class="scan-divider">/</span>
    <div class="scan-stat">
        <span class="scan-value">{formatNumber(scanFilesFound)}</span>
        <span class="scan-label">{t('fileOperations.scanPhase.scanFile', { count: scanFilesFound })}</span>
    </div>
    <span class="scan-divider">/</span>
    <div class="scan-stat">
        <span class="scan-value">{formatNumber(scanDirsFound)}</span>
        <span class="scan-label">{t('fileOperations.scanPhase.scanDir', { count: scanDirsFound })}</span>
    </div>
    <span
        class="scan-status"
        role="img"
        aria-label={tString('fileOperations.shared.scanningTooltip')}
        use:tooltip={{ text: tString('fileOperations.shared.scanningTooltip') }}
    >
        <Spinner size="sm" />
    </span>
</div>

<!-- Throughput -->
{#if scanFilesPerSec !== null && scanFilesPerSec > 0}
    <div class="scan-throughput">
        <span class="scan-throughput-value"
            >{tString('fileOperations.scanPhase.throughputFiles', {
                rateText: formatNumber(Math.round(scanFilesPerSec)),
            })}</span
        >
        {#if scanBytesPerSec !== null && scanBytesPerSec > 0}
            <span class="scan-throughput-sep">·</span>
            <span class="scan-throughput-value"><Trans key="fileOperations.shared.byteRate" snippets={{ size }} /></span
            >
        {/if}
    </div>
{/if}

<!-- Current directory + filename -->
{#if scanCurrentDir}
    <div class="scan-current-dir" use:useShortenMiddle={{ text: scanCurrentDir, preferBreakAt: '/' }}></div>
{/if}
{#if currentFile}
    <div class="current-file" use:useShortenMiddle={{ text: currentFile, preferBreakAt: '/' }}></div>
{/if}

{#snippet size(children: import('svelte').Snippet)}<Size bytes={scanBytesPerSec ?? 0} />{@render children()}{/snippet}

<style>
    .source-path {
        display: flex;
        align-items: baseline;
        justify-content: center;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        overflow: hidden;
    }

    .source-path-label {
        flex-shrink: 0;
    }

    .source-path-value {
        flex: 1;
        min-width: 0;
        overflow: hidden;
        white-space: nowrap;
    }

    .scan-wait-stats {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-sm);
        font-size: var(--font-size-sm);
    }

    .scan-status {
        display: inline-flex;
        align-items: center;
    }

    .scan-throughput {
        display: flex;
        justify-content: center;
        gap: var(--spacing-xs);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .scan-throughput-value {
        font-variant-numeric: tabular-nums;
    }

    .scan-throughput-sep {
        opacity: 0.6;
    }

    .scan-current-dir {
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        overflow: hidden;
        white-space: nowrap;
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-sm);
    }

    .scan-stat {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-xs);
    }

    .scan-value {
        color: var(--color-text-primary);
        font-variant-numeric: tabular-nums;
        font-weight: 500;
    }

    .scan-label {
        color: var(--color-text-tertiary);
    }

    .scan-divider {
        color: var(--color-text-tertiary);
    }

    /* Current file styles - shared with parent but scoped here for the snippet portion */
    .current-file {
        padding: var(--spacing-sm) var(--spacing-xl);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        overflow: hidden;
        white-space: nowrap;
        background: var(--color-bg-tertiary);
        margin: 0 var(--spacing-lg);
        border-radius: var(--radius-sm);
    }
</style>
