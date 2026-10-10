<script lang="ts">
    /**
     * A switcher row's disk-space sub-line: the usage bar and the short figure, or, while
     * the space query is slow, a retrying spinner or a click-to-retry placeholder. The
     * fetching and retry state are `volume-space-manager.svelte.ts`'s; this only shows them.
     */
    import { tString } from '$lib/intl/messages.svelte'
    import { getFileSizeFormat } from '$lib/settings/reactive-settings.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { getUsageBar, formatDiskSpaceShort } from '../disk-space-utils'
    import type { VolumeInfo } from '../types'
    import type { VolumeSpaceManager } from './volume-space-manager.svelte'

    interface Props {
        volume: VolumeInfo
        spaceManager: VolumeSpaceManager
    }

    const { volume, spaceManager }: Props = $props()

    const space = $derived(spaceManager.volumeSpaceMap.get(volume.id))
</script>

{#if space}
    {@const bar = getUsageBar(space)}
    <div class="volume-space-info">
        <!-- No bar where there is no total to fill it against: the line carries
             the used figure on its own. -->
        {#if bar}
            <div class="volume-space-bar">
                <div
                    class="volume-space-fill"
                    style:width="{bar.usedPercent}%"
                    style:background-color="var({bar.cssVar})"
                ></div>
            </div>
        {/if}
        <span class="volume-space-text">{formatDiskSpaceShort(space, getFileSizeFormat())}</span>
    </div>
{:else if spaceManager.spaceRetryingSet.has(volume.id)}
    <div
        class="volume-space-info volume-space-timeout"
        use:tooltip={spaceManager.spaceAutoRetryingSet.has(volume.id)
            ? tString('fileExplorer.navigation.spaceRetryingAuto')
            : tString('fileExplorer.navigation.spaceRetrying')}
    >
        <div class="volume-space-bar volume-space-bar-timeout"><Spinner size="sm" /></div>
        <span class="volume-space-text volume-space-text-timeout"
            >{tString('fileExplorer.navigation.spaceRetryingText')}</span
        >
    </div>
{:else if spaceManager.spaceTimedOutSet.has(volume.id)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
        class="volume-space-info volume-space-timeout"
        class:space-shake={spaceManager.spaceRetryFailedSet.has(volume.id)}
        use:tooltip={spaceManager.spaceRetryAttemptedSet.has(volume.id)
            ? tString('fileExplorer.navigation.spaceStillUnavailable')
            : tString('fileExplorer.navigation.spaceFetchFailed')}
        onclick={(e: MouseEvent) => {
            e.stopPropagation()
            spaceManager.retryVolumeSpace(volume)
        }}
    >
        <div class="volume-space-bar volume-space-bar-timeout">
            <span class="volume-space-timeout-icon">?</span>
        </div>
        <span class="volume-space-text volume-space-text-timeout"
            >{tString('fileExplorer.navigation.spaceUnavailableText')}</span
        >
    </div>
{/if}

<style>
    .volume-space-info {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- left pad aligns to a computed icon+gap offset; 14px/16px are measured widths */
        padding: 0 var(--spacing-md) var(--spacing-xs) calc(14px + var(--spacing-sm) + 16px + var(--spacing-sm));
    }

    .volume-space-bar {
        flex: 1;
        height: 2px;
        background-color: var(--color-disk-track);
        border-radius: var(--radius-sm);
    }

    .volume-space-fill {
        height: 100%;
        border-radius: var(--radius-sm);
    }

    .volume-space-text {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        white-space: nowrap;
        flex-shrink: 0;
    }

    /* Volume space timeout placeholder */
    .volume-space-timeout {
        cursor: default;
    }

    .volume-space-bar-timeout {
        border: 1px dashed var(--color-border);
        background-color: transparent;
        display: flex;
        align-items: center;
        justify-content: center;
        height: 8px;
    }

    .volume-space-timeout-icon {
        font-size: var(--font-size-xs);
        color: var(--color-warning);
        line-height: var(--font-line-height-flat);
        transition: opacity var(--transition-base);
    }

    /* Shake on retry failure */
    /*noinspection CssUnusedSymbol*/
    .space-shake {
        animation: shake 300ms ease;
    }

    @keyframes shake {
        0%,
        100% {
            transform: translateX(0);
        }
        25% {
            transform: translateX(-3px);
        }
        75% {
            transform: translateX(3px);
        }
    }

    .volume-space-text-timeout {
        color: var(--color-warning);
    }

    @keyframes flash-warning {
        0%,
        100% {
            background-color: transparent;
        }
        50% {
            background-color: var(--color-warning-bg);
        }
    }

    @media (prefers-reduced-motion: reduce) {
        /* Reduced motion: opacity flash instead of shake */
        /*noinspection CssUnusedSymbol*/
        .space-shake {
            animation: flash-warning 300ms ease;
        }
    }
</style>
