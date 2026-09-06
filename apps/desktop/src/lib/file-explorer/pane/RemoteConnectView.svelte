<script lang="ts">
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
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
     * ❌ No inert buttons. A refusal offers Try again (which really re-dials) and,
     * where there is a session to drop, Disconnect. The "Sign in…" button lands
     * with the sheet that can answer it, not before.
     */
    interface Props {
        /** The place's display name, for the sentence and the aria live region. */
        name: string
        state: RemoteConnectState
    }

    const { name, state }: Props = $props()
</script>

<div class="remote-connect" role="status" aria-live="polite">
    <div class="content">
        <div class="place-name">{name}</div>
        {#if state.kind === 'connecting'}
            <h2 class="title">{tString('servers.paneState.connecting', { name })}</h2>
            <div class="spinner-row"><Spinner size="md" /></div>
            <p class="hint">{tString('servers.paneState.connectingHint')}</p>
            <div class="actions">
                <Button variant="secondary" size="mini" onclick={state.cancel}>
                    {tString('servers.paneState.cancel')}
                </Button>
            </div>
        {:else}
            <span class="refusal-icon"><Icon name="triangle-alert" size={32} aria-hidden="true" /></span>
            <h2 class="title">{state.refusal}</h2>
            <div class="actions">
                <Button variant="primary" size="mini" onclick={state.retry}>
                    {tString('servers.paneState.tryAgain')}
                </Button>
                {#if state.disconnect}
                    <Button variant="secondary" size="mini" onclick={state.disconnect}>
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

    .actions {
        display: flex;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-sm);
    }
</style>
