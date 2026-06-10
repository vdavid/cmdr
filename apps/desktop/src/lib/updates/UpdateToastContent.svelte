<script lang="ts">
    import { relaunch } from '@tauri-apps/plugin-process'
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'

    function handleRestart() {
        void relaunch()
    }

    function handleDismiss() {
        dismissToast('update')
    }
</script>

<span class="update-text">New version available. Restart to update.</span>
<div class="update-actions">
    <Button variant="secondary" size="mini" onclick={handleDismiss}>Later</Button>
    <Button variant="primary" size="mini" onclick={handleRestart}>Restart</Button>
</div>

<style>
    .update-text {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .update-actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
