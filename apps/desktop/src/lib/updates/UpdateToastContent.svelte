<script lang="ts">
    import { relaunch } from '@tauri-apps/plugin-process'
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { updateState } from './update-state.svelte'

    /**
     * Both ends of the version row, or `null` when either is unknown. A toast raised from `ready`
     * always has both (`previousVersion` is set when a check starts, `nextVersion` when an update
     * is found), so this is a guard rather than a real branch: it keeps a half-populated state from
     * rendering `v → v0.29.0` instead of dropping the row.
     */
    const versions = $derived(
        updateState.previousVersion !== null && updateState.nextVersion !== null
            ? { prev: updateState.previousVersion, next: updateState.nextVersion }
            : null,
    )

    function handleRestart() {
        void relaunch()
    }

    function handleDismiss() {
        dismissToast('update')
    }
</script>

<div class="update-body">
    <span class="update-headline">{tString('updates.toast.ready')}</span>
    <span class="update-detail">{tString('updates.toast.readyDetail')}</span>
    {#if versions !== null}
        <!-- `role="img"` so the arrow is announced as the sentence in `versionChangeAria` rather
             than read out as a bare symbol, in whatever language the screen reader names it. -->
        <span class="update-versions" role="img" aria-label={tString('updates.toast.versionChangeAria', versions)}>
            {tString('updates.toast.versionChange', versions)}
        </span>
    {/if}
</div>
<div class="update-actions">
    <Button variant="secondary" size="mini" onclick={handleDismiss}>{tString('updates.toast.later')}</Button>
    <Button variant="primary" size="mini" onclick={handleRestart}>{tString('updates.toast.restart')}</Button>
</div>

<style>
    .update-body {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .update-headline {
        font-size: var(--font-size-sm);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .update-detail {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .update-versions {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        /* Version numbers line up under each other across re-renders instead of jittering. */
        font-variant-numeric: tabular-nums;
    }

    .update-actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
