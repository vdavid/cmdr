<script lang="ts">
    /**
     * Preview-and-send dialog for user-initiated error reports (Flow A).
     *
     * Mounted from `(main)/+layout.svelte` and driven by the reactive `errorReportFlow`
     * store. Calls `prepareErrorReportPreview` to render the preview and `sendErrorReport`
     * to ship the bundle. In dev, an extra "Save bundle to disk (debug)" button writes
     * the zip to the app data dir for inspection.
     */
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { addToast } from '$lib/ui/toast'
    import {
        prepareErrorReportPreview,
        sendErrorReport,
        saveErrorReportToDisk,
        type PreviewPayload,
    } from '$lib/tauri-commands/error-reporter'
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call -- Svelte module export type not resolved
    import ErrorReportToastContent, { setLastSentReportId } from './ErrorReportToastContent.svelte'
    import { closeErrorReportDialog, errorReportFlow } from './error-report-flow.svelte'
    import { getAppLogger } from '$lib/logging/logger'

    const log = getAppLogger('errorReportDialog')

    const MAX_NOTE_CHARS = 100_000
    const SOFT_WARN_AT = 50_000
    const POST_SEND_TOAST_MS = 10_000

    let userNote = $state(errorReportFlow.initialNote)
    let detailsExpanded = $state(false)
    let preview = $state<PreviewPayload | null>(null)
    let preparingError = $state<string | null>(null)
    let preparing = $state(true)
    let sending = $state(false)
    let copiedId = $state(false)

    const noteLength = $derived(userNote.length)
    const noteOverLimit = $derived(noteLength > MAX_NOTE_CHARS)
    const showCounter = $derived(noteLength > SOFT_WARN_AT)
    const isDev = import.meta.env.DEV

    // Re-prepare whenever the user note changes. Debounced to avoid hammering the
    // backend on each keystroke. The manifest size depends on the note, so the
    // preview byte count shifts as the user types.
    let previewTimer: ReturnType<typeof setTimeout> | undefined
    $effect(() => {
        const note = userNote
        clearTimeout(previewTimer)
        previewTimer = setTimeout(() => {
            void refreshPreview(note)
        }, 250)
        return () => {
            clearTimeout(previewTimer)
        }
    })

    async function refreshPreview(note: string) {
        if (note.length > MAX_NOTE_CHARS) {
            // Skip preview while invalid — the manifest would be rejected anyway.
            return
        }
        try {
            const result = await prepareErrorReportPreview(note || undefined)
            preview = result
            preparingError = null
        } catch (e) {
            preparingError = String(e)
            log.warn("Couldn't prepare error report preview: {error}", { error: String(e) })
        } finally {
            preparing = false
        }
    }

    async function handleSend() {
        if (sending || noteOverLimit) return
        sending = true
        try {
            const result = await sendErrorReport(userNote || undefined)
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call -- Svelte module export type not resolved
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
            addToast(`Couldn't send error report: ${String(e)}`, { level: 'error' })
        } finally {
            sending = false
        }
    }

    async function handleSaveToDisk() {
        try {
            const path = await saveErrorReportToDisk(userNote || undefined)
            addToast(`Saved bundle to ${path}`, { level: 'info', timeoutMs: 8000 })
        } catch (e) {
            addToast(`Couldn't save bundle: ${String(e)}`, { level: 'error' })
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

    function handleKeydown(event: KeyboardEvent) {
        // Cmd/Ctrl+Enter sends. Plain Enter is consumed by the textarea.
        if ((event.metaKey || event.ctrlKey) && event.key === 'Enter' && !sending && !noteOverLimit) {
            event.preventDefault()
            void handleSend()
        }
    }

    function formatBytes(bytes: number): string {
        if (bytes < 1024) return `${bytes} B`
        if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
        return `${(bytes / 1024 / 1024).toFixed(2)} MB`
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
    {#snippet title()}Send error report{/snippet}

    <div class="body">
        <p id="error-report-body" class="description">
            This sends Cmdr's recent log files to the team so we can fix what went wrong. The logs
            are redacted client-side — file paths, hostnames, IPs, and emails are scrubbed before
            sending.
        </p>

        {#if preview}
            <div class="id-row">
                <span class="id-label">Reference ID:</span>
                <span class="id-badge">{preview.id}</span>
                <button class="link-button" onclick={() => void handleCopyId()}>
                    {copiedId ? 'Copied' : 'Copy'}
                </button>
            </div>
        {/if}

        <label class="note-label" for="error-report-note">
            <span>Add a note (optional)</span>
            {#if showCounter}
                <span class="note-counter" class:over={noteOverLimit}>
                    {noteLength.toLocaleString('en-US')} / {MAX_NOTE_CHARS.toLocaleString('en-US')}
                </span>
            {/if}
        </label>
        <textarea
            id="error-report-note"
            bind:value={userNote}
            class="note-textarea"
            class:invalid={noteOverLimit}
            placeholder="What were you trying to do? What did you expect to happen?"
            rows="4"
        ></textarea>
        {#if noteOverLimit}
            <p class="helper-text">
                Note is too long. Maximum is {MAX_NOTE_CHARS.toLocaleString('en-US')} characters.
            </p>
        {/if}

        <button
            class="details-toggle"
            onclick={() => (detailsExpanded = !detailsExpanded)}
            aria-expanded={detailsExpanded}
        >
            <span class="toggle-arrow" class:expanded={detailsExpanded}>&#x25B8;</span>
            What's about to be sent
            {#if preview}
                <span class="size-hint">({formatBytes(preview.sizeBytes)})</span>
            {/if}
        </button>

        {#if detailsExpanded && preview}
            <div class="details-container">
                <h3 class="details-heading">Manifest</h3>
                <pre class="details-block">{JSON.stringify(preview.manifest, null, 2)}</pre>

                <h3 class="details-heading">Sample of first {preview.sampleFirst.length} lines</h3>
                <pre class="details-block sample-block">{preview.sampleFirst.join('\n') ||
                        '(no log lines available)'}</pre>

                <h3 class="details-heading">Sample of last {preview.sampleLast.length} lines</h3>
                <pre class="details-block sample-block">{preview.sampleLast.join('\n') ||
                        '(no log lines available)'}</pre>

                <p class="meta-line">
                    Total log lines (after redaction): {preview.totalRedactedLines.toLocaleString(
                        'en-US',
                    )}
                </p>
            </div>
        {/if}

        {#if preparing && !preview}
            <p class="status">Preparing preview…</p>
        {/if}
        {#if preparingError}
            <p class="status status-error">Couldn't prepare preview: {preparingError}</p>
        {/if}

        <div class="button-row">
            {#if isDev}
                <Button variant="secondary" onclick={() => void handleSaveToDisk()} disabled={sending}>
                    Save bundle to disk (debug)
                </Button>
            {/if}
            <span class="spacer"></span>
            <Button variant="secondary" onclick={handleClose} disabled={sending}>Cancel</Button>
            <Button
                variant="primary"
                onclick={() => void handleSend()}
                disabled={sending || noteOverLimit || preparing}
            >
                {sending ? 'Sending…' : 'Send report'}
            </Button>
        </div>
    </div>
</ModalDialog>

<style>
    .body {
        padding: 0 var(--spacing-xl) var(--spacing-xl);
    }

    .description {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.5;
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

    .note-textarea {
        width: 100%;
        font-family: var(--font-system);
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        padding: var(--spacing-sm) var(--spacing-md);
        resize: vertical;
        margin-bottom: var(--spacing-md);
    }

    .note-textarea.invalid {
        border-color: var(--color-error);
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
