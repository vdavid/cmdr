<script lang="ts">
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import SentReportToastBody from './SentReportToastBody.svelte'

    interface Props {
        toastId: string
        /** The report this toast talks about. */
        reportId: string
        /** `sent`: a new report shipped. `amended`: a note joined the report Flow B had already sent. */
        kind: 'sent' | 'amended'
    }

    const { toastId, reportId, kind }: Props = $props()
    let copied = $state(false)

    // One toast, two outcomes: a report that shipped, and a note that joined the report
    // Flow B had already sent. Only the lead sentence differs.
    const messageKey = $derived(
        kind === 'amended'
            ? ('errorReporter.amendedToast.message' as const)
            : ('errorReporter.sentToast.message' as const),
    )

    async function handleCopy() {
        await navigator.clipboard.writeText(reportId)
        copied = true
        setTimeout(() => (copied = false), 2000)
    }

    function handleDismiss() {
        dismissToast(toastId)
    }
</script>

{#snippet actions()}
    <Button size="mini" variant="secondary" onclick={handleDismiss}>{tString('errorReporter.sentToast.dismiss')}</Button>
    <Button size="mini" variant="primary" onclick={() => void handleCopy()}>
        {copied ? tString('errorReporter.sentToast.copied') : tString('errorReporter.sentToast.copyId')}
    </Button>
{/snippet}

<SentReportToastBody message={tString(messageKey)} {reportId} {actions} />
