<script lang="ts">
    /**
     * What a burst of failures collapses into: one notice with the count.
     *
     * It reads the count off the main window's store rather than taking it as a
     * prop, for one mechanical reason: the toast store's dedup path replaces a
     * toast's content and level but NOT its props, so a prop-carried count
     * would freeze at whatever the fourth failure saw. Reading live also keeps
     * it honest while the user clears rows in the queue window.
     */
    import OperationFailureToastBody from './OperationFailureToastBody.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { getMainWindowOperationRows } from '$lib/file-operations/queue/main-window-operations.svelte'

    interface Props {
        /** Dedup id of this toast; lets the component get out of its own way. */
        toastId: string
    }

    const { toastId }: Props = $props()

    const count = $derived(getMainWindowOperationRows().filter((row) => row.snapshot.status === 'failed').length)
    const title = $derived(tString('queue.failureToast.summary', { count, countText: formatInteger(count) }))
</script>

<OperationFailureToastBody {toastId} {title} />
