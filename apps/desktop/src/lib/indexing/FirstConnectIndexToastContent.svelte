<script lang="ts">
    /**
     * First-connect indexing prompt (D6): shown the first time the user opens a
     * NEW external drive, asking whether to index it. Three actions: enable
     * indexing, silence this drive, silence all drives. The caller (the toast
     * trigger) gates whether this even shows; this component just renders the
     * choice and runs the picked action, then self-dismisses.
     */
    import Button from '$lib/ui/Button.svelte'
    import { dismissToast } from '$lib/ui/toast'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        /** Dedup id of this toast; lets the component self-dismiss on a choice. */
        toastId: string
        /** The drive being prompted about. */
        volumeId: string
        /** The drive's display name (for the heading). */
        volumeName: string
        /** Turn on indexing for this drive (kicks off the scan). */
        onEnable: (volumeId: string) => void
        /** Remember "don't ask again for this drive". */
        onSilenceDrive: (volumeId: string) => void
        /** Turn the per-drive prompt off for every drive. */
        onSilenceAll: () => void
    }

    const { toastId, volumeId, volumeName, onEnable, onSilenceDrive, onSilenceAll }: Props = $props()

    function enable() {
        onEnable(volumeId)
        dismissToast(toastId)
    }
    function silenceDrive() {
        onSilenceDrive(volumeId)
        dismissToast(toastId)
    }
    function silenceAll() {
        onSilenceAll()
        dismissToast(toastId)
    }
</script>

<div class="first-connect-toast">
    <span class="title">{tString('indexing.firstConnect.title', { name: volumeName })}</span>
    <p class="body">{tString('indexing.firstConnect.body')}</p>
    <div class="actions">
        <Button variant="primary" size="mini" onclick={enable}>
            {tString('indexing.firstConnect.enable')}
        </Button>
        <Button variant="secondary" size="mini" onclick={silenceDrive}>
            {tString('indexing.firstConnect.silenceDrive')}
        </Button>
        <Button variant="secondary" size="mini" onclick={silenceAll}>
            {tString('indexing.firstConnect.silenceAll')}
        </Button>
    </div>
</div>

<style>
    .first-connect-toast {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .title {
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .body {
        margin: 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .actions {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-xs);
        margin-top: var(--spacing-xxs);
    }
</style>
