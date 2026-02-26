<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { setSetting } from '$lib/settings'

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
    {#snippet title()}Change file extension?{/snippet}

    <p id="extension-change-description" class="description">
        Are you sure you want to change the extension from ".{oldExtension}" to ".{newExtension}"? Your file may open in
        a different app next time you open it.
    </p>

    <label class="always-allow">
        <input type="checkbox" bind:checked={alwaysAllow} />
        <span>Always allow extension changes</span>
    </label>

    <div class="button-row">
        <Button variant="secondary" onclick={onKeepOld}>Keep .{oldExtension}</Button>
        <Button variant="primary" onclick={handleUseNew}>Use .{newExtension}</Button>
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
        gap: 8px;
        padding: 0 24px 16px;
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
        gap: 12px;
        justify-content: center;
        padding: 0 24px 20px;
    }
</style>
