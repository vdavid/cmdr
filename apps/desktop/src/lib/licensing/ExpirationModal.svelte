<script lang="ts">
    import { markExpirationModalShown, openExternalUrl } from '$lib/tauri-commands'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        organizationName: string | null
        expiredAt: string
        onClose: () => void
    }

    const { organizationName, expiredAt, onClose }: Props = $props()

    function formatDate(isoString: string): string {
        try {
            const date = new Date(isoString)
            return date.toLocaleDateString(undefined, { year: 'numeric', month: 'long', day: 'numeric' })
        } catch {
            return expiredAt
        }
    }

    async function handleDismiss() {
        await markExpirationModalShown()
        onClose()
    }

    async function handleRenew() {
        await openExternalUrl('https://getcmdr.com/renew')
    }
</script>

{#snippet strong(children: import('svelte').Snippet)}<strong>{@render children()}</strong>{/snippet}

<ModalDialog
    titleId="modal-title"
    blur
    dialogId="expiration"
    onclose={() => {
        void handleDismiss()
    }}
    containerStyle="max-width: 420px; background: var(--color-bg-primary); border-color: var(--color-border)"
>
    {#snippet title()}{tString('licensing.expiration.title')}{/snippet}

    <div class="modal-body">
        {#if organizationName}
            <p class="org-name">
                <Trans key="licensing.expiration.orgName" params={{ org: organizationName }} snippets={{ strong }} />
            </p>
        {/if}

        <p class="message">
            <Trans key="licensing.expiration.message" params={{ date: formatDate(expiredAt) }} snippets={{ strong }} />
        </p>

        <p class="info">{tString('licensing.expiration.info')}</p>
    </div>

    {#snippet footer()}
        <Button variant="secondary" onclick={handleDismiss}>{tString('licensing.expiration.continue')}</Button>
        <Button variant="primary" onclick={handleRenew}>{tString('licensing.expiration.renew')}</Button>
    {/snippet}
</ModalDialog>

<style>
    .org-name {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }

    .message {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .info {
        margin: 0 0 var(--spacing-xl);
        font-size: var(--font-size-md);
        color: var(--color-text-tertiary);
        line-height: 1.5;
    }
</style>
