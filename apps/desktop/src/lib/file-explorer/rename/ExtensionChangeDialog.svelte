<script lang="ts">
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import Button from '#lib/ui/Button.svelte'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import { setSetting } from '#lib/settings/index.js'
    import { tString } from '#lib/intl/messages.svelte.js'

    interface Props {
        /** The extension before the rename, no dot; `''` when the name had none. */
        oldExtension: string
        /** The extension after it, no dot; `''` when the rename removes it. */
        newExtension: string
        /** A folder (a macOS package, since only those have extensions) words its consequence differently. */
        isDirectory?: boolean
        /** Who started the rename, when it was a guarded caller: they chose the new extension. */
        startedBy?: ToolInitiator
        onKeepOld: () => void
        onUseNew: () => void
    }

    const { oldExtension, newExtension, isDirectory = false, startedBy, onKeepOld, onUseNew }: Props = $props()

    // Three different changes, each worded on its own so nothing ever reads "Use ." or "to “.”".
    const kind = $derived(isDirectory ? 'folder' : 'file')
    const description = $derived(
        !newExtension
            ? tString('fileExplorer.extensionChange.descriptionRemove', { kind, oldExt: oldExtension })
            : !oldExtension
              ? tString('fileExplorer.extensionChange.descriptionAdd', { kind, newExt: newExtension })
              : tString('fileExplorer.extensionChange.description', { oldExt: oldExtension, newExt: newExtension }),
    )
    const keepLabel = $derived(
        oldExtension
            ? tString('fileExplorer.extensionChange.keepOld', { oldExt: oldExtension })
            : tString('fileExplorer.extensionChange.keepNone'),
    )
    const useLabel = $derived(
        newExtension
            ? tString('fileExplorer.extensionChange.useNew', { newExt: newExtension })
            : tString('fileExplorer.extensionChange.removeExtension', { oldExt: oldExtension }),
    )

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
    {startedBy}
    role="alertdialog"
    ariaDescribedby="extension-change-description"
    onkeydown={handleKeydown}
    onclose={onKeepOld}
    containerStyle="min-width: 380px; max-width: 460px"
>
    {#snippet title()}{tString('fileExplorer.extensionChange.title')}{/snippet}

    <p id="extension-change-description" class="description">
        {description}
    </p>

    <div class="always-allow">
        <Checkbox bind:checked={alwaysAllow}>{tString('fileExplorer.extensionChange.alwaysAllow')}</Checkbox>
    </div>

    {#snippet footer()}
        <Button variant="secondary" onclick={onKeepOld}>{keepLabel}</Button>
        <Button variant="primary" onclick={handleUseNew}>{useLabel}</Button>
    {/snippet}
</ModalDialog>

<style>
    .description {
        margin: 0;
        padding: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }

    .always-allow {
        padding-bottom: var(--spacing-lg);
        color: var(--color-text-secondary);
    }
</style>
