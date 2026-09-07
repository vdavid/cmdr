<script lang="ts">
    import { onDestroy } from 'svelte'
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import ProgressBar from '$lib/ui/ProgressBar.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { tString } from '$lib/intl/messages.svelte'
    import type { RemoteConnectState } from './remote-connect-state'

    /**
     * The pane, while a remote place is on its way in or has stopped short.
     *
     * Panes are for waiting; dialogs are for entering data
     * (`docs/specs/servers-hub-plan.md` § "The four rules"). This view is the
     * waiting half, and it holds no state of its own: the caller hands it a
     * typed `state` and the callbacks that go with it.
     *
     * ❗ **Every state says what is happening in one sentence, and every state
     * with a process behind it has a cancel.** A spinner with no words is the
     * shape of failure this replaces: the user can't tell a slow handshake from
     * a wedged one, and has no way out of either.
     *
     * ❌ No inert buttons. A refusal offers Try again (which really re-dials),
     * "Signed out" offers Sign in… (which opens the sheet), and a changed host
     * key offers Disconnect (which is what lets the next open show the
     * fingerprint). Every one of them does the thing it says.
     */
    interface Props {
        /** The place's display name, for the sentence and the aria live region. */
        name: string
        state: RemoteConnectState
    }

    // ❗ Renamed on the way in: a local binding called `state` would make every
    // `$state(...)` in this file read as a store subscription instead of a rune.
    const { name, state: connectState }: Props = $props()

    /** The backoff loop's face, when a loop is what's running. */
    const cycle = $derived(connectState.kind === 'connecting' ? connectState.cycle : undefined)
    const waiting = $derived(cycle?.waiting ?? null)

    /**
     * The countdown bar, 0..1 over the current wait. Animated with
     * `requestAnimationFrame` so the bar drains smoothly without re-rendering
     * the rest of the view on every frame.
     */
    let progress = $state(0)
    let rafId: number | null = null

    function tick() {
        if (!waiting) {
            progress = 0
            rafId = null
            return
        }
        progress = Math.min(1, (performance.now() - waiting.startedAt) / waiting.durationMs)
        rafId = progress < 1 ? requestAnimationFrame(tick) : null
    }

    $effect(() => {
        // Re-arm on each new wait. Reading both fields is what wires the effect
        // to the phase change.
        const armed = waiting ? `${String(waiting.startedAt)}:${String(waiting.durationMs)}` : null
        void armed
        if (rafId !== null) cancelAnimationFrame(rafId)
        rafId = null
        progress = 0
        if (waiting) rafId = requestAnimationFrame(tick)
    })

    onDestroy(() => {
        if (rafId !== null) cancelAnimationFrame(rafId)
    })
</script>

<div class="remote-connect" role="status" aria-live="polite">
    <div class="content">
        <div class="place-name">{name}</div>
        {#if connectState.kind === 'connecting'}
            <h2 class="title">
                {cycle
                    ? tString('servers.paneState.reconnecting', { name })
                    : tString('servers.paneState.connecting', { name })}
            </h2>
            <div class="spinner-row"><Spinner size="md" /></div>
            {#if cycle}
                <div class="progress-row">
                    {#if waiting}
                        <ProgressBar
                            value={progress}
                            ariaLabel={tString('servers.paneState.retryProgressAriaLabel')}
                        />
                    {:else}
                        <!-- An attempt is in flight: the spinner carries the motion. -->
                        <div class="progress-placeholder"></div>
                    {/if}
                </div>
                <!-- Keyed by position, ❌ not by text: the cycle's own sentences
                     are the list, and a locale where two of them came out
                     identical would throw on a text key. -->
                {#each cycle.lines as line, i (i)}
                    <p class="hint">{line}</p>
                {/each}
            {:else}
                <p class="hint">{tString('servers.paneState.connectingHint')}</p>
            {/if}
            <div class="actions">
                {#if cycle}
                    <span use:tooltip={tString('servers.paneState.retryNowTooltip')}>
                        <Button variant="primary" size="mini" onclick={cycle.retryNow} disabled={!waiting}>
                            {tString('servers.paneState.retryNow')}
                        </Button>
                    </span>
                {/if}
                <span use:tooltip={cycle ? tString('servers.paneState.cancelCycleTooltip') : undefined}>
                    <Button variant="secondary" size="mini" onclick={connectState.cancel}>
                        {tString('servers.paneState.cancel')}
                    </Button>
                </span>
                {#if cycle}
                    <span use:tooltip={tString('servers.paneState.disconnectCycleTooltip')}>
                        <Button variant="secondary" size="mini" onclick={cycle.disconnect}>
                            {tString('servers.paneState.disconnect')}
                        </Button>
                    </span>
                {/if}
            </div>
        {:else if connectState.kind === 'signed_out'}
            <span class="refusal-icon"><Icon name="lock" size={32} aria-hidden="true" /></span>
            <h2 class="title">{tString('servers.paneState.signedOut', { name })}</h2>
            {#if connectState.signIn}
                <div class="actions">
                    <Button variant="primary" size="mini" onclick={connectState.signIn}>
                        {tString('servers.paneState.signIn')}
                    </Button>
                </div>
            {:else}
                <!-- ❌ No button: the backend's shape says there is no secret a
                     person could type that would bring this session back. -->
                <p class="hint">{tString('servers.paneState.signedOutNothingToAsk')}</p>
            {/if}
        {:else if connectState.kind === 'host_key_changed'}
            <span class="refusal-icon danger"><Icon name="triangle-alert" size={32} aria-hidden="true" /></span>
            <h2 class="title">{tString('servers.paneState.hostKeyChanged', { name })}</h2>
            <p class="hint">{tString('servers.paneState.hostKeyChangedHint')}</p>
            <div class="actions">
                <Button variant="secondary" size="mini" onclick={connectState.disconnect}>
                    {tString('servers.paneState.disconnect')}
                </Button>
            </div>
        {:else}
            <span class="refusal-icon"><Icon name="triangle-alert" size={32} aria-hidden="true" /></span>
            <h2 class="title">{connectState.refusal}</h2>
            <div class="actions">
                <Button variant="primary" size="mini" onclick={connectState.retry}>
                    {tString('servers.paneState.tryAgain')}
                </Button>
                {#if connectState.disconnect}
                    <Button variant="secondary" size="mini" onclick={connectState.disconnect}>
                        {tString('servers.paneState.disconnect')}
                    </Button>
                {/if}
            </div>
        {/if}
    </div>
</div>

<style>
    .remote-connect {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        padding: var(--spacing-xl);
    }

    .content {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: var(--spacing-md);
        max-width: 420px;
        text-align: center;
    }

    .place-name {
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

    .hint {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .refusal-icon {
        display: inline-flex;
        align-items: center;
        color: var(--color-warning);
    }

    /* A changed host key is the shape a man-in-the-middle takes, so it carries
       more weight than an ordinary refusal. */
    .refusal-icon.danger {
        color: var(--color-error);
    }

    .actions {
        display: flex;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-sm);
    }
</style>
