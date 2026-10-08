<script lang="ts">
    /**
     * The new-folder / new-file dialogs' notice while a slow volume is still
     * creating the entry (`create-submission.svelte.ts`, phase `stillCreating`).
     * `role="status"` announces it without taking focus from the name field.
     */
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        /** The name being created. */
        name: string
    }

    const { name }: Props = $props()
</script>

<p class="still-creating" role="status">
    <Spinner size="sm" />
    <span>{tString('fileOperations.newEntry.stillCreating', { name })}</span>
</p>

<style>
    .still-creating {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-sm);
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }
</style>
