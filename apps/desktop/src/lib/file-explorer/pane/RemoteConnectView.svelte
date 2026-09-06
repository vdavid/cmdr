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
        {:else if state.kind === 'signed_out'}
            <span class="refusal-icon"><Icon name="lock" size={32} aria-hidden="true" /></span>
            <h2 class="title">{tString('servers.paneState.signedOut', { name })}</h2>
            <div class="actions">
                <Button variant="primary" size="mini" onclick={state.signIn}>
                    {tString('servers.paneState.signIn')}
                </Button>
            </div>
        {:else if state.kind === 'host_key_changed'}
            <span class="refusal-icon danger"><Icon name="triangle-alert" size={32} aria-hidden="true" /></span>
            <h2 class="title">{tString('servers.paneState.hostKeyChanged', { name })}</h2>
            <p class="hint">{tString('servers.paneState.hostKeyChangedHint')}</p>
            <div class="actions">
                <Button variant="secondary" size="mini" onclick={state.disconnect}>
                    {tString('servers.paneState.disconnect')}
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
