<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { setSetting } from '$lib/settings'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        oldExtension: string
        newExtension: string
        onKeepOld: () => void
        onUseNew: () => void
    }

    const { oldExtension, newExtension, onKeepOld, onUseNew }: Props = $props()

    let alwaysAllow = $state(false)

    function handleUseNew() {
        if (alwaysAllow) {
            setSetting('fileOperations.allowFileExtensionChanges', 'yes')
        }
        onUseNew()
    }

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            event.preventDefault()
            handleUseNew()
        }
    }
</script>

<ModalDialog
    titleId="extension-change-title"
    dialogId="extension-change"
    role="alertdialog"
    ariaDescribedby="extension-change-description"
    onkeydown={handleKeydown}
    onclose={onKeepOld}
    containerStyle="min-width: 380px; max-width: 460px"
>
    {#snippet title()}{tString('fileExplorer.extensionChange.title')}{/snippet}

    <p id="extension-change-description" class="description">
        {tString('fileExplorer.extensionChange.description', { oldExt: oldExtension, newExt: newExtension })}
    </p>

    <label class="always-allow">
        <input type="checkbox" bind:checked={alwaysAllow} />
        <span>{tString('fileExplorer.extensionChange.alwaysAllow')}</span>
    </label>

    <div class="button-row">
        <Button variant="secondary" onclick={onKeepOld}>{tString('fileExplorer.extensionChange.keepOld', { oldExt: oldExtension })}</Button>
        <Button variant="primary" onclick={handleUseNew}>{tString('fileExplorer.extensionChange.useNew', { newExt: newExtension })}</Button>
    </div>
</ModalDialog>

<style>
    .description {
        margin: 0;
        padding: 0 var(--spacing-xl) var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .always-allow {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: 0 var(--spacing-xl) var(--spacing-lg);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        cursor: default;
    }

    .always-allow input[type='checkbox'] {
        margin: 0;
        cursor: default;
    }

    .button-row {
        display: flex;
        gap: var(--spacing-md);
        justify-content: center;
        padding: 0 var(--spacing-xl) var(--spacing-xl);
    }
</style>
