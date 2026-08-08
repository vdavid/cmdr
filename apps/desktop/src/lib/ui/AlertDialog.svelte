<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import CopyBox from '$lib/ui/CopyBox.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        title: string
        message: string
        /**
         * A path the alert is ABOUT (the one that's too long, the one that vanished).
         * Rendered below the message as selectable, copyable monospace text rather
         * than buried in the sentence, because it's the payload the user has to act
         * on. Also widens the panel (see `PATH_WIDTH_PX`).
         */
        path?: string
        buttonText?: string
        onClose: () => void
    }

    const { title: dialogTitle, message, path, buttonText, onClose }: Props = $props()
    const resolvedButtonText = $derived(buttonText ?? tString('ui.alertDialog.defaultButton'))

    /** Alert panel width: wide enough for a sentence or two, no wider. */
    const WIDTH_PX = 420
    /**
     * A path alert gets 1.5× that. The payload is one long unbreakable string, so
     * every extra pixel is one less wrapped line, and the alert is about length in
     * the first place.
     */
    const PATH_WIDTH_PX = 630

    /**
     * Display cap for the path. macOS tops out at 1024 bytes per path, so this only
     * ever bites a pathological string (a synthetic name, a mangled URL) that would
     * otherwise stretch the dialog into a wall of text. The clipboard still gets the
     * whole thing.
     */
    const MAX_PATH_DISPLAY_CHARS = 1000

    /** Middle-truncated so the filename survives: the tail carries the meaning. */
    const displayPath = $derived.by(() => {
        if (path === undefined || path.length <= MAX_PATH_DISPLAY_CHARS) return undefined
        const keep = MAX_PATH_DISPLAY_CHARS - 1
        const head = Math.ceil(keep / 2)
        return `${path.slice(0, head)}…${path.slice(path.length - (keep - head))}`
    })

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            onClose()
        }
    }
</script>

<ModalDialog
    titleId="alert-dialog-title"
    onkeydown={handleKeydown}
    role="alertdialog"
    dialogId="alert"
    onclose={onClose}
    ariaDescribedby="alert-dialog-message"
    containerStyle="width: {path === undefined ? WIDTH_PX : PATH_WIDTH_PX}px"
    resizable="horizontal"
>
    {#snippet title()}{dialogTitle}{/snippet}

    <p id="alert-dialog-message" class="message">{message}</p>

    {#if path !== undefined}
        <div class="path">
            <CopyBox text={path} displayText={displayPath} copyAriaLabel={tString('ui.alertDialog.copyPathAria')} />
        </div>
    {/if}

    {#snippet footer()}
        <Button variant="primary" onclick={onClose}>{resolvedButtonText}</Button>
    {/snippet}
</ModalDialog>

<style>
    .message {
        margin: 0;
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }

    .path {
        margin-top: var(--spacing-md);
    }
</style>
