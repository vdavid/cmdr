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
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import { addToast } from '$lib/ui/toast'
    import { sendFeedback, openExternalUrl } from '$lib/tauri-commands'
    import { formatInteger } from '$lib/intl/number-format'
    import { t, tString } from '$lib/intl/messages.svelte'
    import Trans from '$lib/intl/Trans.svelte'
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
                addToast(tString('feedback.sentToast'), { level: 'success' })
                feedbackText = ''
                closeFeedbackDialog()
            } else if (result.kind === 'invalid') {
                // Both empty and over-cap are blocked above, so this is a backstop.
                sendFailedMessage = tString('feedback.dialog.invalid')
            } else {
                sendFailedMessage = tString('feedback.dialog.softFailure')
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

{#snippet githubLink(children: import('svelte').Snippet)}
    <LinkButton
        href={GITHUB_ISSUES_URL}
        onclick={(e: MouseEvent) => {
            e.preventDefault()
            void handleOpenLink(GITHUB_ISSUES_URL)
        }}>{@render children()}</LinkButton
    >
{/snippet}
{#snippet callLink(children: import('svelte').Snippet)}
    <LinkButton
        href={BOOK_A_CALL_URL}
        onclick={(e: MouseEvent) => {
            e.preventDefault()
            void handleOpenLink(BOOK_A_CALL_URL)
        }}>{@render children()}</LinkButton
    >
{/snippet}

<ModalDialog
    titleId="feedback-dialog-title"
    onkeydown={handleKeydown}
    dialogId="feedback"
    role="dialog"
    onclose={handleClose}
    ariaDescribedby="feedback-dialog-body"
    containerStyle="width: 480px"
>
    {#snippet title()}{tString('feedback.dialog.title')}{/snippet}

    <div>
        <p id="feedback-dialog-body" class="description">
            {tString('feedback.dialog.description')}
        </p>

        <label class="feedback-label" for="feedback-text">
            <span>{tString('feedback.dialog.label')}</span>
            {#if showCounter}
                <span class="counter" class:over={overLimit}>
                    {t('feedback.dialog.counter', {
                        currentText: formatInteger(textLength),
                        maxText: formatInteger(MAX_FEEDBACK_CHARS),
                    })}
                </span>
            {/if}
        </label>
        <textarea
            id="feedback-text"
            bind:this={textareaRef}
            bind:value={feedbackText}
            class="feedback-textarea"
            class:invalid={overLimit}
            placeholder={tString('feedback.dialog.placeholder')}
            rows="5"
        ></textarea>
        {#if overLimit}
            <p class="helper-text">
                {t('feedback.dialog.tooLong', { maxText: formatInteger(MAX_FEEDBACK_CHARS) })}
            </p>
        {/if}

        {#if contactEmail}
            <div class="attach-email">
                <Checkbox bind:checked={attachEmail}>{t('feedback.dialog.attachEmail', { email: contactEmail })}</Checkbox>
            </div>
        {/if}

        <p class="more-ways">
            <Trans key="feedback.dialog.moreWays" snippets={{ github: githubLink, call: callLink }} />
        </p>

        {#if sendFailedMessage}
            <p class="status status-error" role="alert">{sendFailedMessage}</p>
        {/if}
    </div>

    {#snippet footer()}
        <Button variant="secondary" onclick={handleClose} disabled={sending}>{tString('feedback.dialog.cancel')}</Button>
        <Button variant="primary" onclick={() => void handleSend()} disabled={sending || isEmpty || overLimit}>
            {sending ? tString('feedback.dialog.sending') : tString('feedback.dialog.send')}
        </Button>
    {/snippet}
</ModalDialog>

<style>
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
        margin-bottom: var(--spacing-md);
        color: var(--color-text-secondary);
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
</style>
