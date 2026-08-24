<script lang="ts">
    // The status-corner indicator for the proactive agent: a wake thinking right now (with a way
    // in and a way to stop it), or the gate standing between the agent and noticing anything.
    //
    // A plain inline box, because the corner owns placement (`status-corner/CLAUDE.md`). It reads
    // module `$state` and opens no subscription of its own: `wake-indicator.svelte.ts` holds the
    // listener, and `StatusCorner`'s two test suites mount this for real.
    //
    // Which states render, and which stay silent, is `wakeIndicatorMode`'s decision — the one
    // place the "a gap is worth reporting" and "a control with nothing to say is noise" rules are
    // reconciled.
    import Icon from '$lib/ui/Icon.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { tString } from '$lib/intl/messages.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { openWakeThread, stopWake, wakeIndicator, wakeIndicatorMode } from './wake-indicator.svelte'

    const log = getAppLogger('askCmdr')

    const mode = $derived(wakeIndicatorMode(wakeIndicator))
    const gapLabel = $derived(
        mode === 'needsFullDiskAccess'
            ? tString('askCmdr.wake.needsFullDiskAccess')
            : tString('askCmdr.wake.needsApiKey'),
    )

    /** Send the user where the gap is closed: the FDA pane, or the provider settings. */
    async function openGapFix(): Promise<void> {
        // Imported lazily so the status corner doesn't drag the settings window and the privacy
        // shim into every test that mounts it.
        if (mode === 'needsFullDiskAccess') {
            const { openPrivacySettings } = await import('$lib/tauri-commands')
            await openPrivacySettings()
            return
        }
        const { openSettingsWindow } = await import('$lib/settings/settings-window')
        await openSettingsWindow('wake-indicator', ['AI', 'Provider'])
    }

    function warn(what: string): (error: unknown) => void {
        return (error: unknown) => {
            log.warn('{what} from the wake indicator failed: {error}', { what, error: String(error) })
        }
    }
</script>

{#if mode === 'thinking'}
    <span class="wake-indicator">
        <button
            class="glyph thinking"
            onclick={() => void openWakeThread().catch(warn('opening the wake thread'))}
            use:tooltip={tString('askCmdr.wake.thinking')}
            aria-label={tString('askCmdr.wake.thinking')}
        >
            <Icon name="bot" size={14} />
        </button>
        <button
            class="glyph stop"
            onclick={() => void stopWake().catch(warn('stopping the wake'))}
            use:tooltip={tString('askCmdr.wake.stop')}
            aria-label={tString('askCmdr.wake.stop')}
        >
            <Icon name="square" size={10} />
        </button>
    </span>
{:else if mode !== 'silent'}
    <span class="wake-indicator">
        <button
            class="glyph gap"
            onclick={() => void openGapFix().catch(warn('opening the settings behind the gap'))}
            use:tooltip={gapLabel}
            aria-label={gapLabel}
        >
            <Icon name="brain-circuit" size={14} />
        </button>
    </span>
{/if}

<style>
    .wake-indicator {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xxs);
        height: 20px;
        padding: 0 var(--spacing-xs);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
    }

    .glyph {
        display: inline-flex;
        align-items: center;
        border: none;
        background: none;
        padding: 0;
        color: inherit;
    }

    .glyph:hover {
        color: var(--color-text-primary);
    }

    /* A wake is spending the user's money right now, so the glyph has to read as active rather
       than as one more permanent badge. Opacity only, and frozen under reduced motion: the corner
       sits over the pane and a moving shape there is exactly what that setting is asking us not
       to do. */
    .thinking {
        animation: wake-pulse 1.8s ease-in-out infinite;
    }

    @keyframes wake-pulse {
        0%,
        100% {
            opacity: 1;
        }
        50% {
            opacity: 0.4;
        }
    }

    @media (prefers-reduced-motion: reduce) {
        .thinking {
            animation: none;
        }
    }

    /* The gap is a nudge, not a failure: the agent is watching and will catch up the moment the
       user closes it. Warning text, never error. */
    .gap {
        color: var(--color-warning-text);
    }
</style>
