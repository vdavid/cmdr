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
    import Size from '$lib/ui/Size.svelte'
    import { addToast } from '$lib/ui/toast'
    import {
        prepareErrorReportPreview,
        sendErrorReport,
        saveErrorReportToDisk,
        type PreviewPayload,
    } from '$lib/tauri-commands/error-reporter'
     
    import ErrorReportToastContent, { setLastSentReportId } from './ErrorReportToastContent.svelte'
    import BundleSavedToastContent, { setLastSavedBundlePath } from './BundleSavedToastContent.svelte'
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
            const result = await prepareErrorReportPreview(undefined)
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
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call -- Svelte module export type not resolved
            setLastSavedBundlePath(path)
            addToast(BundleSavedToastContent, {
                id: 'error-report-bundle-saved',
                level: 'success',
                dismissal: 'persistent',
            })
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
                <span class="size-hint">(<Size bytes={preview.sizeBytes} />)</span>
            {/if}
        </button>

        {#if detailsExpanded && preview}
            <div class="details-container">
                <h3 class="details-heading">Manifest</h3>
                <pre class="details-block">{JSON.stringify(displayedManifest, null, 2)}</pre>

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
