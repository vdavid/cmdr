<script lang="ts">
    /**
     * "Send feedback" dialog for the open beta.
     *
     * Mounted from `(main)/+layout.svelte` and driven by the reactive `feedbackFlow`
     * store, mirroring `ErrorReportDialog`. Ships the text via the `send_feedback`
     * IPC command; no log bundle rides along (that's the error reporter's job).
     */
    import { onMount, tick } from 'svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import { addToast } from '$lib/ui/toast'
    import { sendFeedback, openExternalUrl } from '$lib/tauri-commands'
    import { closeFeedbackDialog } from './feedback-flow.svelte'
    import { getSetting, setSetting } from '$lib/settings'
    import { getAppLogger } from '$lib/logging/logger'
    import { GITHUB_ISSUES_URL, BOOK_A_CALL_URL } from '$lib/beta-links'

    const log = getAppLogger('feedbackDialog')

    // Same caps as the error reporter's note textarea (and the backend + server validators).
    const MAX_FEEDBACK_CHARS = 100_000
    const SOFT_WARN_AT = 50_000

    let feedbackText = $state('')
    let textareaRef: HTMLTextAreaElement | undefined
    // Beta contact email (if set) and the sticky attach-email choice. The checkbox shows
    // only when an email is on file; never pre-ticked on first use (default false). Shares
    // `updates.attachEmailToReports` with the error and crash report dialogs, so the
    // choice sticks across all three.
    const contactEmail = getSetting('analytics.email').trim()
    let attachEmail = $state(getSetting('updates.attachEmailToReports'))
    const emailToAttach = $derived(attachEmail && contactEmail ? contactEmail : undefined)
    let sending = $state(false)
    let sendFailedMessage = $state<string | null>(null)

    // Count by Unicode code points so the frontend cap matches the Rust validator's
    // `.chars().count()` and the server's `[...text].length`. `feedbackText.length`
    // (UTF-16 code units) diverges for surrogate-pair characters (most emoji).
    const textLength = $derived(Array.from(feedbackText).length)
    const overLimit = $derived(textLength > MAX_FEEDBACK_CHARS)
    const showCounter = $derived(textLength > SOFT_WARN_AT)
    const isEmpty = $derived(feedbackText.trim().length === 0)

    async function handleSend() {
        if (sending || isEmpty || overLimit) return
        sending = true
        sendFailedMessage = null
        try {
            if (contactEmail) {
                setSetting('updates.attachEmailToReports', attachEmail)
            }
            const result = await sendFeedback(feedbackText, emailToAttach)
            if (result.kind === 'sent') {
                addToast('Thanks for the feedback! We read every note.', { level: 'success' })
                feedbackText = ''
                closeFeedbackDialog()
            } else if (result.kind === 'invalid') {
                // Both empty and over-cap are blocked above, so this is a backstop.
                sendFailedMessage = "That note didn't go through. Shorten it and try again?"
            } else {
                sendFailedMessage = "Sorry, we couldn't send your feedback right now. Try again?"
            }
        } finally {
            sending = false
        }
    }

    async function handleOpenLink(url: string) {
        try {
            await openExternalUrl(url)
        } catch (e) {
            log.warn("Couldn't open external link: {error}", { error: String(e) })
        }
    }

    function handleClose() {
        closeFeedbackDialog()
    }

    onMount(async () => {
        // Focus the textarea so the user can type immediately (keyboard-first). After a tick
        // so it wins over ModalDialog's overlay focus, which runs in the child's onMount.
        await tick()
        textareaRef?.focus()
    })

    function handleKeydown(event: KeyboardEvent) {
        // Cmd/Ctrl+Enter sends. Plain Enter is consumed by the textarea.
        if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
            event.preventDefault()
            void handleSend()
        }
    }
</script>

<ModalDialog
    titleId="feedback-dialog-title"
    onkeydown={handleKeydown}
    dialogId="feedback"
    role="dialog"
    onclose={handleClose}
    ariaDescribedby="feedback-dialog-body"
    containerStyle="width: 480px"
>
    {#snippet title()}Send feedback{/snippet}

    <div class="body">
        <p id="feedback-dialog-body" class="description">
            What's working? What's missing? Your note goes straight to the maker of Cmdr.
        </p>

        <label class="feedback-label" for="feedback-text">
            <span>Your feedback</span>
            {#if showCounter}
                <span class="counter" class:over={overLimit}>
                    {textLength.toLocaleString('en-US')} / {MAX_FEEDBACK_CHARS.toLocaleString(
                        'en-US',
                    )}
                </span>
            {/if}
        </label>
        <textarea
            id="feedback-text"
            bind:this={textareaRef}
            bind:value={feedbackText}
            class="feedback-textarea"
            class:invalid={overLimit}
            placeholder="Example: I'd love a shortcut for jumping between tabs."
            rows="5"
        ></textarea>
        {#if overLimit}
            <p class="helper-text">
                Feedback is too long. Maximum is {MAX_FEEDBACK_CHARS.toLocaleString('en-US')} characters.
            </p>
        {/if}

        {#if contactEmail}
            <label class="attach-email">
                <input type="checkbox" bind:checked={attachEmail} />
                <span>Attach my email ({contactEmail}) so we can reply</span>
            </label>
        {/if}

        <p class="more-ways">
            You can also <LinkButton
                href={GITHUB_ISSUES_URL}
                onclick={(e: MouseEvent) => {
                    e.preventDefault()
                    void handleOpenLink(GITHUB_ISSUES_URL)
                }}>browse and vote on GitHub</LinkButton
            > or <LinkButton
                href={BOOK_A_CALL_URL}
                onclick={(e: MouseEvent) => {
                    e.preventDefault()
                    void handleOpenLink(BOOK_A_CALL_URL)
                }}>book a call</LinkButton
            > with the maker.
        </p>

        {#if sendFailedMessage}
            <p class="status status-error" role="alert">{sendFailedMessage}</p>
        {/if}

        <div class="button-row">
            <span class="spacer"></span>
            <Button variant="secondary" onclick={handleClose} disabled={sending}>Cancel</Button>
            <Button
                variant="primary"
                onclick={() => void handleSend()}
                disabled={sending || isEmpty || overLimit}
            >
                {sending ? 'Sending…' : 'Send feedback'}
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

    .feedback-label {
        display: flex;
        justify-content: space-between;
        align-items: baseline;
        margin-bottom: var(--spacing-xs);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .counter {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .counter.over {
        color: var(--color-error);
    }

    .feedback-textarea {
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

    .feedback-textarea.invalid {
        border-color: var(--color-error);
    }

    .helper-text {
        margin: calc(var(--spacing-md) * -1) 0 var(--spacing-md);
        font-size: var(--font-size-xs);
        color: var(--color-error);
    }

    .attach-email {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        margin-bottom: var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        cursor: default;
    }

    .attach-email input[type='checkbox'] {
        accent-color: var(--color-accent);
    }

    .more-ways {
        margin: 0 0 var(--spacing-md);
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
