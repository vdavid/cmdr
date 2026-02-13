<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
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
        <button class="secondary" onclick={onKeepOld}>Keep .{oldExtension}</button>
        <button class="primary" onclick={handleUseNew}>Use .{newExtension}</button>
    </div>
</ModalDialog>

<style>
    .description {
        margin: 0;
        padding: 0 24px 16px;
        font-size: 13px;
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .always-allow {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 0 24px 16px;
        font-size: 12px;
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

    button {
        padding: 8px 20px;
        border-radius: 6px;
        font-size: 13px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.15s ease;
        min-width: 80px;
    }

    .primary {
        background: var(--color-accent);
        color: white;
        border: none;
    }

    .primary:hover {
        filter: brightness(1.1);
    }

    .secondary {
        background: transparent;
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border-primary);
    }

    .secondary:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }
</style>
