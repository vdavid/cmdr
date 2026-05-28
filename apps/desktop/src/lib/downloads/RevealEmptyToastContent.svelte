<script module lang="ts">
    /**
     * Module-level slots for the action callback + dismiss id. The reveal
     * helper assigns these right before `addToast(RevealEmptyToastContent, ...)`
     * so the "Go to Downloads" button can act without props (the toast store's
     * `ToastContent` type doesn't carry props).
     *
     * Same prop-bridging pattern as `BundleSavedToastContent`. The callback is
     * a closure over the focused-pane + Downloads dir captured at toast-add
     * time, so a remap of the focused pane after the toast appears doesn't
     * change the destination — the user sees the same destination they would
     * have seen when they pressed ⌘J.
     */
    let goToDownloads = $state<(() => void) | null>(null)

    export function setEmptyToastHandler(action: () => void): void {
        goToDownloads = action
    }
</script>

<script lang="ts">
    import { dismissToast } from '$lib/ui/toast'
    import { REVEAL_EMPTY_TOAST_ID } from './reveal-ids'

    function handleGoToDownloads() {
        goToDownloads?.()
        dismissToast(REVEAL_EMPTY_TOAST_ID)
    }

    function handleDismiss() {
        dismissToast(REVEAL_EMPTY_TOAST_ID)
    }
</script>

<div class="content">
    <span class="message">Your Downloads folder is empty. Go there anyway?</span>
    <div class="actions">
        <button class="link-button" onclick={handleGoToDownloads}>Go to Downloads</button>
        <button class="link-button" onclick={handleDismiss}>Dismiss</button>
    </div>
</div>

<style>
    .content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .message {
        color: var(--color-text-primary);
        line-height: 1.4;
    }

    .actions {
        display: flex;
        gap: var(--spacing-md);
    }

    .link-button {
        background: none;
        border: none;
        padding: 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .link-button:hover {
        color: var(--color-text-secondary);
    }
</style>
