<script lang="ts">
    /**
     * What a whole window shows when its content broke: a calm heading, what to do, the
     * throw's `Name: message` under "Technical details", a report offer, and Reload.
     *
     * Two hosts render it: `routes/+error.svelte` (SvelteKit's error page, for a page that
     * threw while rendering) and `WindowBoundary` (the root layout's boundary, for anything
     * that got past every inner boundary). Either way the main window's layout, which
     * normally hosts the report dialog, is gone, so in the main window this panel hosts the
     * dialog and the toasts its send confirms through. Other windows ask the main window,
     * as everywhere else.
     */
    import { onMount, tick } from 'svelte'
    import { getCurrentWindow } from '@tauri-apps/api/window'
    import Button from '#lib/ui/Button.svelte'
    import ToastContainer from '#lib/ui/toast/ToastContainer.svelte'
    import ErrorReportDialog from './ErrorReportDialog.svelte'
    import ReportOffer from './ReportOffer.svelte'
    import { errorReportFlow, hostErrorReportDialog } from './error-report-flow.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'

    interface Props {
        /** The throw's `Name: message`, or `null` when nothing readable came with it. */
        detail: string | null
        onReload: () => void
    }

    const { detail, onReload }: Props = $props()

    let isMainWindow = $state(false)
    let headingRef: HTMLHeadingElement | undefined

    onMount(() => {
        // Focus lands on the heading, so a screen reader says what happened and Tab reaches the buttons.
        void tick().then(() => headingRef?.focus())
        isMainWindow = getCurrentWindow().label === 'main'
        if (!isMainWindow) return
        return hostErrorReportDialog()
    })
</script>

<main class="page-crash">
    <h1 bind:this={headingRef} tabindex="-1">{tString('errorReporter.pageCrash.title')}</h1>
    <p class="body">{tString('errorReporter.pageCrash.body')}</p>
    {#if detail}
        <section class="details" aria-labelledby="page-crash-details-label">
            <h2 id="page-crash-details-label">{tString('errorReporter.pageCrash.detailsLabel')}</h2>
            <pre>{detail}</pre>
        </section>
    {/if}
    <div class="actions">
        <ReportOffer incident={{ kind: 'pageCrash', detail }} />
        <Button variant="primary" onclick={onReload}>{tString('errorReporter.pageCrash.reload')}</Button>
    </div>
</main>

{#if isMainWindow}
    <ToastContainer />
    {#if errorReportFlow.open}
        <ErrorReportDialog />
    {/if}
{/if}

<style>
    .page-crash {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
        max-width: 560px;
        margin: 0 auto;
        padding: var(--spacing-2xl) var(--spacing-xl);
        color: var(--color-text-primary);
    }

    h1 {
        margin: 0;
        font-size: var(--font-size-xl);
        outline: none;
    }

    .body {
        margin: 0;
        color: var(--color-text-secondary);
    }

    h2 {
        margin: 0 0 var(--spacing-xs);
        font-size: var(--font-size-sm);
        font-weight: normal;
        color: var(--color-text-secondary);
    }

    pre {
        margin: 0;
        padding: var(--spacing-sm) var(--spacing-md);
        border-radius: var(--radius-md);
        background: var(--color-bg-secondary);
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
        white-space: pre-wrap;
        overflow-wrap: anywhere;
        user-select: text;
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
    }
</style>
