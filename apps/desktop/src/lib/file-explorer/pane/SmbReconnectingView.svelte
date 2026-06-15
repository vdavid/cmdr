<script lang="ts">
    import { onDestroy } from 'svelte'
    import Button from '$lib/ui/Button.svelte'
    import ProgressBar from '$lib/ui/ProgressBar.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import {
        smbReconnectManager,
        TOTAL_DURATION_MS,
        reconnectProgressMessage,
        type ReconnectState,
    } from '../network/smb-reconnect-manager.svelte'

    interface Props {
        volumeId: string
        /** Display name of the share (e.g. "naspi"). Used in the spoken/aria label. */
        shareName: string
        /** Cycle state (read by the parent from the manager and passed down so the parent can also key on it). */
        cycleState: ReconnectState
        onCancel: () => void
        onDisconnect: () => void
    }

    const { volumeId, shareName, cycleState, onCancel, onDisconnect }: Props = $props()

    // Progress bar: 0..1, animated over `cycleState.currentDelayMs` from
    // `cycleState.waitStartedAt`. Uses requestAnimationFrame so the bar drains
    // smoothly without forcing a re-render of the rest of the manager state.
    let progress = $state(0)
    let rafId: number | null = null

    function tick() {
        if (cycleState.status !== 'waiting') {
            progress = 0
            rafId = null
            return
        }
        const elapsed = performance.now() - cycleState.waitStartedAt
        progress = Math.min(1, elapsed / cycleState.currentDelayMs)
        if (progress < 1) {
            rafId = requestAnimationFrame(tick)
        } else {
            rafId = null
        }
    }

    $effect(() => {
        // Re-arm the animation whenever a new `waiting` phase starts.
        // Reading `cycleState.status`, `cycleState.waitStartedAt`, and `cycleState.currentDelayMs`
        // wires the effect to re-run on each phase change.
        const _statusDep = cycleState.status
        const _startedDep = cycleState.waitStartedAt
        const _delayDep = cycleState.currentDelayMs
        void _statusDep
        void _startedDep
        void _delayDep
        if (rafId !== null) cancelAnimationFrame(rafId)
        if (cycleState.status === 'waiting') {
            rafId = requestAnimationFrame(tick)
        } else {
            progress = 0
        }
    })

    onDestroy(() => {
        if (rafId !== null) cancelAnimationFrame(rafId)
    })

    function handleRetryNow() {
        smbReconnectManager.retryNow(volumeId)
    }

    /** Total cycle duration as a human sentence ("60 seconds", "2 minutes"). */
    function formatTotalDuration(ms: number): string {
        const seconds = Math.round(ms / 1000)
        if (seconds < 90) return `${String(seconds)} seconds`
        const minutes = Math.round(seconds / 60)
        return `${String(minutes)} minutes`
    }

    const totalDurationLabel = $derived(formatTotalDuration(TOTAL_DURATION_MS))
    const bodyLine2 = $derived(reconnectProgressMessage(cycleState.attemptIndex))
    const isAttempting = $derived(cycleState.status === 'attempting')
</script>

<div class="reconnect-pane" role="status" aria-live="polite">
    <div class="reconnect-content">
        <div class="share-context">{shareName}</div>
        <h2 class="title">Reconnecting to server…</h2>

        <div class="spinner-row">
            <Spinner size="md" />
        </div>

        <div class="progress-row">
            {#if cycleState.status === 'waiting'}
                <ProgressBar value={progress} ariaLabel="Time until next reconnect attempt" />
            {:else}
                <!-- Hide the bar during `attempting`/`gave-up`; spinner carries the motion. -->
                <div class="progress-placeholder"></div>
            {/if}
        </div>

        <p class="body-line-1">Will keep trying for a total of {totalDurationLabel}.</p>
        {#if bodyLine2}
            <p class="body-line-2">{bodyLine2}</p>
        {/if}

        <div class="actions">
            <span use:tooltip={'Try reconnecting immediately.'}>
                <Button variant="primary" size="mini" onclick={handleRetryNow} disabled={isAttempting}>
                    Retry now
                </Button>
            </span>
            <span use:tooltip={'Stop trying for now. The connection stays available. Switch back to retry.'}>
                <Button variant="secondary" size="mini" onclick={onCancel}>Cancel</Button>
            </span>
            <span use:tooltip={'Stop trying and disconnect from the server.'}>
                <Button variant="secondary" size="mini" onclick={onDisconnect}>Disconnect</Button>
            </span>
        </div>
    </div>
</div>

<style>
    .reconnect-pane {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        padding: var(--spacing-xl);
    }

    .reconnect-content {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: var(--spacing-md);
        max-width: 420px;
        text-align: center;
    }

    .share-context {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    .title {
        font-size: var(--font-size-lg);
        font-weight: 500;
        color: var(--color-text-primary);
        margin: 0;
    }

    .spinner-row {
        display: flex;
        justify-content: center;
        margin-top: var(--spacing-sm);
    }

    .progress-row {
        width: 240px;
        display: flex;
        align-items: center;
    }

    .progress-placeholder {
        height: 8px;
        width: 100%;
    }

    .body-line-1 {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .body-line-2 {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .actions {
        display: flex;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-sm);
    }
</style>
