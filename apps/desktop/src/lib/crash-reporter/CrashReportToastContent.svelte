<script lang="ts">
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { openSettingsWindow } from '$lib/settings/settings-window'
    import { tString } from '$lib/intl/messages.svelte'
    import { crashSentToastKey } from './crash-copy'
    import type { CrashReport } from '$lib/tauri-commands'

    // The report that was just auto-sent. It decides whether this toast may say "crash":
    // a panic the app walked away from didn't crash it. `./crash-copy.ts`.
    const { report }: { report: CrashReport } = $props()

    function handleOpenSettings() {
        dismissToast('crash-report-sent')
        void openSettingsWindow('crash-toast')
    }
</script>

<div class="content">
    <span class="message">{tString(crashSentToastKey(report))}</span>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={handleOpenSettings}
            >{tString('crashReporter.sentToast.changeSettings')}</Button
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
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
