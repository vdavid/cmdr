<script lang="ts">
    import { dismissToast } from '$lib/ui/toast'
    import { openSettingsWindow } from '$lib/settings/settings-window'

    function handleOpenSettings() {
        dismissToast('crash-report-sent')
        void openSettingsWindow()
    }
</script>

<span class="toast-text">
    Crash report sent — thanks for helping improve Cmdr.
    <button class="opt-out-link" onclick={handleOpenSettings}> Change in Settings &gt; Updates. </button>
</span>

<style>
    .toast-text {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .opt-out-link {
        background: none;
        border: none;
        padding: 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        cursor: default;
        display: block;
        margin-top: var(--spacing-xs);
    }

    .opt-out-link:hover {
        color: var(--color-text-secondary);
    }
</style>
