<script lang="ts">
    /**
     * Preview-and-send dialog for user-initiated error reports (Flow A).
     *
     * Mounted from `(main)/+layout.svelte` and driven by the reactive `errorReportFlow`
     * store. Calls `prepareErrorReportPreview` to render the preview and `sendErrorReport`
     * to ship the bundle. In dev, an extra "Save bundle to disk (debug)" button writes
     * the zip to the app data dir for inspection.
     */
    import { onMount, tick } from 'svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import AttachEmailCheckbox from '$lib/attach-email/AttachEmailCheckbox.svelte'
    import { createAttachEmail } from '$lib/attach-email/attach-email.svelte'
    import TextArea from '$lib/ui/TextArea.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import Size from '$lib/ui/Size.svelte'
    import { addToast } from '$lib/ui/toast'
    import {
        prepareErrorReportPreview,
        sendErrorReport,
        saveErrorReportToDisk,
        type PreviewPayload,
    } from '$lib/tauri-commands/error-reporter'
     
    import ErrorReportToastContent from './ErrorReportToastContent.svelte'
    import BundleSavedToastContent from './BundleSavedToastContent.svelte'
    import { setLastSentReportId } from './error-report-toast-state.svelte'
    import { setLastSavedBundlePath } from './bundle-saved-toast-state.svelte'
    import { closeErrorReportDialog, errorReportFlow } from './error-report-flow.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { t, tString } from '$lib/intl/messages.svelte'

    const log = getAppLogger('errorReportDialog')

    let noteTextareaRef: HTMLTextAreaElement | undefined

    onMount(async () => {
        // Focus the note textarea so the user can type immediately (keyboard-first). After a
        // tick so it wins over ModalDialog's overlay focus, which runs in the child's onMount.
        await tick()
        noteTextareaRef?.focus()
    })

    const MAX_NOTE_CHARS = 100_000
    const SOFT_WARN_AT = 50_000
    const POST_SEND_TOAST_MS = 10_000

    let userNote = $state(errorReportFlow.initialNote)
    const attachEmail = createAttachEmail()
    let detailsExpanded = $state(false)
    let preview = $state<PreviewPayload | null>(null)
    let preparingError = $state<string | null>(null)
    let preparing = $state(true)
    let sending = $state(false)
    let copiedId = $state(false)

    // Count by Unicode code points so the frontend cap matches the Rust-side
    // `.chars().count()` validator. `userNote.length` is UTF-16 code units, which
    // diverges for surrogate-pair characters (most emoji). See `validate_user_note`
    // in `commands/error_reporter.rs`.
    const noteLength = $derived(countCodePoints(userNote))
    const noteOverLimit = $derived(noteLength > MAX_NOTE_CHARS)
    const showCounter = $derived(noteLength > SOFT_WARN_AT)
    const isDev = import.meta.env.DEV

    // Build the preview ONCE when the dialog mounts. The user note doesn't change the
    // log content: only the manifest's `userNote` field changes, so there's no reason to
    // re-run the megabyte-scale bundle build on every keystroke. The displayed manifest
    // is overlaid with the live `userNote` value below; the actual bundle that gets
    // shipped is rebuilt server-side-of-IPC on Send with the final note.
    $effect(() => {
        void buildInitialPreview()
    })

    async function buildInitialPreview() {
        try {
            const result = await prepareErrorReportPreview(undefined, attachEmail.emailToAttach)
            preview = result
            preparingError = null
        } catch (e) {
            preparingError = String(e)
            log.warn("Couldn't prepare error report preview: {error}", { error: String(e) })
        } finally {
            preparing = false
        }
    }

    // Manifest shown in the preview: the cached one from `buildInitialPreview`, with
    // the live note value patched in so the user sees what they're about to send. The
    // backend trims and drops empty notes before writing them to `manifest.json`, so
    // mirror that here.
    const displayedManifest = $derived.by(() => {
        if (!preview) return null
        const trimmed = userNote.trim()
        return {
            ...preview.manifest,
            userNote: trimmed.length > 0 ? trimmed : undefined,
            // Overlay the live attach-email choice so toggling the checkbox updates the
            // preview without rebuilding the multi-MB bundle. The actual send rebuilds
            // with the final value.
            email: attachEmail.emailToAttach,
        }
    })

    function countCodePoints(s: string): number {
        // Array.from iterates by code point (handles surrogate pairs as one char).
        return Array.from(s).length
    }

    async function handleSend() {
        if (sending || noteOverLimit) return
        sending = true
        try {
            attachEmail.persist()
            const result = await sendErrorReport(userNote || undefined, attachEmail.emailToAttach)
            setLastSentReportId(result.id)
            addToast(ErrorReportToastContent, {
                id: 'error-report-sent',
                level: 'success',
                dismissal: 'transient',
                timeoutMs: POST_SEND_TOAST_MS,
            })
            closeErrorReportDialog()
        } catch (e) {
            log.warn('Sending error report returned an error: {error}', { error: String(e) })
            // Suppress the inline "Send error report…" action: this toast is the failure
            // of that very flow, so offering to re-run it would be a confusing loop.
            addToast(tString('errorReporter.dialog.sendFailedToast', { error: String(e) }), {
                level: 'error',
                suppressErrorReportAction: true,
            })
        } finally {
            sending = false
        }
    }

    async function handleSaveToDisk() {
        try {
            const path = await saveErrorReportToDisk(userNote || undefined, attachEmail.emailToAttach)
            setLastSavedBundlePath(path)
            addToast(BundleSavedToastContent, {
                id: 'error-report-bundle-saved',
                level: 'success',
                dismissal: 'persistent',
            })
        } catch (e) {
            addToast(tString('errorReporter.dialog.saveFailedToast', { error: String(e) }), { level: 'error' })
        }
    }

    async function handleCopyId() {
        if (!preview) return
        await navigator.clipboard.writeText(preview.id)
        copiedId = true
        setTimeout(() => (copiedId = false), 2000)
    }

    function handleClose() {
        closeErrorReportDialog()
    }

    /**
     * Exactly ⌘/⌃Enter, no extra modifiers: ⌥⌘Enter and ⇧⌘Enter are different combos
     * and must not send a report on their way somewhere else.
     */
    function isSendCombo(event: KeyboardEvent): boolean {
        return (event.metaKey || event.ctrlKey) && !event.altKey && !event.shiftKey && event.key === 'Enter'
    }

    function handleKeydown(event: KeyboardEvent) {
        // Cmd/Ctrl+Enter sends. Plain Enter is consumed by the textarea.
        if (isSendCombo(event) && !sending && !noteOverLimit) {
            event.preventDefault()
            void handleSend()
        }
    }

</script>

<ModalDialog
    titleId="error-report-dialog-title"
    onkeydown={handleKeydown}
    dialogId="error-report"
    role="dialog"
    onclose={handleClose}
    ariaDescribedby="error-report-body"
    containerStyle="width: 540px"
>
    {#snippet title()}{tString('errorReporter.dialog.title')}{/snippet}

    <div>
        <p id="error-report-body" class="description">
            {tString('errorReporter.dialog.description')}
        </p>

        {#if preview}
            <div class="id-row">
                <span class="id-label">{tString('errorReporter.dialog.referenceIdLabel')}</span>
                <span class="id-badge">{preview.id}</span>
                <button class="link-button" onclick={() => void handleCopyId()}>
                    {copiedId ? tString('errorReporter.dialog.copied') : tString('errorReporter.dialog.copy')}
                </button>
            </div>
        {/if}

        <label class="note-label" for="error-report-note">
            <span>{tString('errorReporter.dialog.noteLabel')}</span>
            {#if showCounter}
                <span class="note-counter" class:over={noteOverLimit}>
                    {t('errorReporter.dialog.counter', {
                        currentText: formatInteger(noteLength),
                        maxText: formatInteger(MAX_NOTE_CHARS),
                    })}
                </span>
            {/if}
        </label>
        <TextArea
            id="error-report-note"
            bind:textareaElement={noteTextareaRef}
            bind:value={userNote}
            invalid={noteOverLimit}
            radius="md"
            rows={4}
            containerStyle="margin-bottom: var(--spacing-md)"
            placeholder={tString('errorReporter.dialog.notePlaceholder')}
        />
        {#if noteOverLimit}
            <p class="helper-text">
                {t('errorReporter.dialog.noteTooLong', { maxText: formatInteger(MAX_NOTE_CHARS) })}
            </p>
        {/if}

        <AttachEmailCheckbox email={attachEmail} />

        <button
            class="details-toggle"
            onclick={() => (detailsExpanded = !detailsExpanded)}
            aria-expanded={detailsExpanded}
        >
            <span class="toggle-arrow" class:expanded={detailsExpanded}>&#x25B8;</span>
            {tString('errorReporter.dialog.detailsToggle')}
            {#if preview}
                <span class="size-hint">(<Size bytes={preview.sizeBytes} />)</span>
            {/if}
        </button>

        {#if detailsExpanded && preview}
            <div class="details-container">
                <h3 class="details-heading">{tString('errorReporter.dialog.manifestHeading')}</h3>
                <pre class="details-block">{JSON.stringify(displayedManifest, null, 2)}</pre>

                <h3 class="details-heading">
                    {t('errorReporter.dialog.sampleFirstHeading', { count: preview.sampleFirst.length })}
                </h3>
                <pre class="details-block sample-block">{preview.sampleFirst.join('\n') ||
                        tString('errorReporter.dialog.noLogLines')}</pre>

                <h3 class="details-heading">
                    {t('errorReporter.dialog.sampleLastHeading', { count: preview.sampleLast.length })}
                </h3>
                <pre class="details-block sample-block">{preview.sampleLast.join('\n') ||
                        tString('errorReporter.dialog.noLogLines')}</pre>

                <p class="meta-line">
                    {t('errorReporter.dialog.totalLines', {
                        countText: formatInteger(preview.totalRedactedLines),
                    })}
                </p>
            </div>
        {/if}

        {#if preparing && !preview}
            <p class="status">{tString('errorReporter.dialog.preparing')}</p>
        {/if}
        {#if preparingError}
            <p class="status status-error">
                {t('errorReporter.dialog.prepareFailed', { error: preparingError })}
            </p>
        {/if}

        <div class="button-row">
            {#if isDev}
                <Button variant="secondary" onclick={() => void handleSaveToDisk()} disabled={sending}>
                    {tString('errorReporter.dialog.saveToDisk')}
                </Button>
            {/if}
            <span class="spacer"></span>
            <Button variant="secondary" onclick={handleClose} disabled={sending}
                >{tString('errorReporter.dialog.cancel')}</Button
            >
            <Button
                variant="primary"
                onclick={() => void handleSend()}
                disabled={sending || noteOverLimit || preparing}
            >
                {sending ? tString('errorReporter.dialog.sending') : tString('errorReporter.dialog.send')}
            </Button>
        </div>
    </div>
</ModalDialog>

<style>
    .description {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }

    .id-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        margin-bottom: var(--spacing-md);
        padding: var(--spacing-sm) var(--spacing-md);
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-md);
    }

    .id-label {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .id-badge {
        font-family: var(--font-mono);
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
        font-weight: 600;
        flex: 1;
    }

    .link-button {
        background: none;
        border: none;
        padding: 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        cursor: default;
    }

    .link-button:hover {
        color: var(--color-text-primary);
    }

    .note-label {
        display: flex;
        justify-content: space-between;
        align-items: baseline;
        margin-bottom: var(--spacing-xs);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .note-counter {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .note-counter.over {
        color: var(--color-error);
    }

    .helper-text {
        margin: calc(var(--spacing-md) * -1) 0 var(--spacing-md);
        font-size: var(--font-size-xs);
        color: var(--color-error);
    }

    .details-toggle {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        background: none;
        border: none;
        padding: 0;
        margin-bottom: var(--spacing-sm);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        cursor: default;
    }

    .details-toggle:hover {
        color: var(--color-text-primary);
    }

    .toggle-arrow {
        display: inline-block;
        transition: transform var(--transition-base);
    }

    .toggle-arrow.expanded {
        transform: rotate(90deg);
    }

    .size-hint {
        color: var(--color-text-tertiary);
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
    }

    .details-container {
        max-height: 320px;
        overflow-y: auto;
        margin-bottom: var(--spacing-md);
        padding: var(--spacing-sm) var(--spacing-md);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-md);
    }

    .details-heading {
        margin: var(--spacing-sm) 0 var(--spacing-xs);
        font-size: var(--font-size-xs);
        text-transform: uppercase;
        letter-spacing: 0.05em;
        color: var(--color-text-tertiary);
    }

    .details-heading:first-child {
        margin-top: 0;
    }

    .details-block {
        margin: 0 0 var(--spacing-sm);
        padding: var(--spacing-sm);
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-sm);
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        white-space: pre-wrap;
        word-break: break-all;
        user-select: text;
    }

    .sample-block {
        max-height: 140px;
        overflow-y: auto;
    }

    .meta-line {
        margin: var(--spacing-sm) 0 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .status {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .status-error {
        color: var(--color-error);
    }

    .button-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-md);
    }

    .spacer {
        flex: 1;
    }
</style>
