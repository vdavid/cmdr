<script lang="ts">
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { openSettingsWindow } from '$lib/settings/settings-window'
    import { tString } from '$lib/intl/messages.svelte'
    import { pastedAsFileMessage } from './pane/paste-clipboard-as-file-message'
    import type { PastedClipboardFile } from '$lib/tauri-commands'

    interface Props {
        filename: string
        kind: PastedClipboardFile['kind']
        /** Injected by the toast store so the Settings button can self-dismiss. */
        toastId: string
    }

    const { filename, kind, toastId }: Props = $props()

    function handleOpenSettings() {
        dismissToast(toastId)
        void openSettingsWindow(['Behavior', 'Navigation & file ops'])
    }
</script>

<div class="content">
    <span class="message">{pastedAsFileMessage(kind, filename)}</span>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={handleOpenSettings}
            >{tString('fileExplorer.clipboard.pastedAsFileSettings')}</Button
        >
    </div>
</div>

<style>
    .content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .message {
        color: var(--color-text-primary);
        line-height: 1.4;
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
