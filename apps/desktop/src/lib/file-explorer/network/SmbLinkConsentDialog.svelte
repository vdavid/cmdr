<script lang="ts">
    /**
     * "Sign in to another server?", opened from the error screen of a pane standing
     * on a folder behind a link to another server.
     *
     * The one place a person allows that sign-in, so the link's target can't collect
     * credentials from a share they merely opened: `crates/cmdr-smb/DETAILS.md`
     * § "Signing in through a link". ❌ Enter doesn't confirm: this hands a password
     * to another machine, so it takes a deliberate press.
     */
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import Button from '#lib/ui/Button.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { allowSmbLinkTarget } from '#lib/tauri-commands/index.js'
    import { getAppLogger } from '#lib/logging/logger.js'

    const log = getAppLogger('fileExplorer')

    interface Props {
        /** The server the share is on. */
        server: string
        /** The other server the link leads to. */
        target: string
        /** The sign-in is allowed and remembered, so the listing can run again. */
        onAllowed: () => void
        onCancel: () => void
    }

    const { server, target, onAllowed, onCancel }: Props = $props()
    /** The message is the alertdialog's description, read with its title. */
    const uid = $props.id()
    const messageId = `${uid}-message`

    let isAllowing = $state(false)

    async function allow(): Promise<void> {
        isAllowing = true
        try {
            if (await allowSmbLinkTarget(server, target)) {
                onAllowed()
                return
            }
            log.warn('The backend refused to allow an SMB link sign-in')
        } catch (error) {
            log.warn('Could not allow an SMB link sign-in: {error}', { error })
        }
        isAllowing = false
        onCancel()
    }
</script>

<ModalDialog
    titleId="smb-link-consent-title"
    dialogId="smb-link-consent"
    role="alertdialog"
    ariaDescribedby={messageId}
    onclose={onCancel}
    containerStyle="width: 460px"
>
    {#snippet title()}{tString('fileExplorer.smbLinkConsent.title')}{/snippet}
    <div id={messageId} class="body">
        <p>{tString('fileExplorer.smbLinkConsent.message', { server, target })}</p>
        <p>{tString('fileExplorer.smbLinkConsent.warning', { target })}</p>
        <p class="note">{tString('fileExplorer.smbLinkConsent.note')}</p>
    </div>
    {#snippet footer()}
        <Button variant="secondary" disabled={isAllowing} onclick={onCancel}>
            {tString('fileExplorer.smbLinkConsent.notNow')}
        </Button>
        <Button variant="primary" disabled={isAllowing} onclick={() => void allow()}>
            {tString('fileExplorer.smbLinkConsent.allow')}
        </Button>
    {/snippet}
</ModalDialog>

<style>
    .body {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        font-size: var(--font-size-md);
        line-height: var(--font-line-height-prose);
        color: var(--color-text-secondary);
        overflow-wrap: anywhere;
    }

    .body p {
        margin: 0;
    }

    .note {
        font-size: var(--font-size-sm);
    }
</style>
