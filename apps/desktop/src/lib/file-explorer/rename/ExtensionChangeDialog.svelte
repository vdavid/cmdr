<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Checkbox from '$lib/ui/Checkbox.svelte'
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

    <div class="always-allow">
        <Checkbox bind:checked={alwaysAllow}>{tString('fileExplorer.extensionChange.alwaysAllow')}</Checkbox>
    </div>

    {#snippet footer()}
        <Button variant="secondary" onclick={onKeepOld}>{tString('fileExplorer.extensionChange.keepOld', { oldExt: oldExtension })}</Button>
        <Button variant="primary" onclick={handleUseNew}>{tString('fileExplorer.extensionChange.useNew', { newExt: newExtension })}</Button>
    {/snippet}
</ModalDialog>

<style>
    .description {
        margin: 0;
        padding: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .always-allow {
        padding-bottom: var(--spacing-lg);
        color: var(--color-text-secondary);
    }
</style>
