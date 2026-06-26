<script lang="ts">
    /**
     * One-time INFO toast shown the first time a double-click on the pane
     * background navigates to the parent folder. Explains what happened and
     * lets the user keep the behavior ("I like it", primary) or turn it off
     * ("Never do this again", which flips
     * `behavior.doubleClickPaneNavigatesToParent` to false and live-applies).
     *
     * The pane sets `behavior.doubleClickOnPaneNotificationSeen` to true when it
     * raises this toast, so it only ever appears once.
     */
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { setSetting } from '$lib/settings'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        toastId: string
    }

    const { toastId }: Props = $props()

    function handleNeverAgain(): void {
        setSetting('behavior.doubleClickPaneNavigatesToParent', false)
        dismissToast(toastId)
    }

    function handleILikeIt(): void {
        dismissToast(toastId)
    }
</script>

<div class="content">
    <strong class="title">{tString('fileExplorer.doubleClickHint.title')}</strong>
    <span class="body">{tString('fileExplorer.doubleClickHint.body')}</span>
    <div class="actions">
        <span class="prompt">{tString('fileExplorer.doubleClickHint.dontLikeIt')}</span>
        <Button size="mini" variant="secondary" onclick={handleNeverAgain}
            >{tString('fileExplorer.doubleClickHint.neverAgain')}</Button
        >
        <Button size="mini" variant="primary" onclick={handleILikeIt}
            >{tString('fileExplorer.doubleClickHint.iLikeIt')}</Button
        >
    </div>
</div>

<style>
    .content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .title {
        color: var(--color-text-primary);
        font-weight: 600;
    }

    .body {
        color: var(--color-text-primary);
        line-height: 1.4;
    }

    .actions {
        display: flex;
        align-items: center;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-sm);
    }

    .prompt {
        margin-right: auto;
        color: var(--color-text-secondary);
    }
</style>
