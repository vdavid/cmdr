<script lang="ts">
    import Button from '$lib/ui/Button.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import type { MessageKey } from '$lib/intl/keys.gen'
    import type { StalledOn } from '$lib/ipc/bindings'

    /**
     * What a pane shows while its listing is stalled: the folder's volume has gone
     * quiet mid-read (`listing-stalled`). The backend keeps waiting and retrying, so
     * the spinner stays: this is a wait with words, not an error screen. Go back is
     * the same step as Esc during a load.
     */
    interface Props {
        folderPath: string
        /** What the folder lives on, as the backend proved it from the mount: picks the wording. */
        stalledOn: StalledOn
        onRetry: () => void
        onGoBack: () => void
    }

    const { folderPath, stalledOn, onRetry, onGoBack }: Props = $props()

    // `unknown` (a phone, a FUSE or cloud mount) keeps the line that names both.
    const detailKey: Record<StalledOn, MessageKey> = {
        server: 'fileExplorer.listingStalled.detailServer',
        drive: 'fileExplorer.listingStalled.detailDrive',
        unknown: 'fileExplorer.listingStalled.detail',
    }
</script>

<div class="stalled" role="status" aria-live="polite">
    <div class="content">
        <Spinner size="lg" />
        <h2 class="title">{tString('fileExplorer.listingStalled.title')}</h2>
        <p class="folder-path">{folderPath}</p>
        <p class="detail">{tString(detailKey[stalledOn])}</p>
        <div class="actions">
            <Button variant="primary" onclick={onRetry}>{tString('fileExplorer.listingStalled.tryAgain')}</Button>
            <Button onclick={onGoBack}>
                {tString('fileExplorer.listingStalled.goBack')}
                <ShortcutChip key="Esc" size="sm" />
            </Button>
        </div>
    </div>
</div>

<style>
    .stalled {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        padding: var(--spacing-xl);
        line-height: var(--font-line-height-prose);
    }

    .content {
        display: flex;
        flex-direction: column;
        align-items: center;
        max-width: 450px;
        text-align: center;
    }

    .title {
        font-size: var(--font-size-xl);
        font-weight: 600;
        margin: var(--spacing-lg) 0 var(--spacing-sm) 0;
        color: var(--color-accent-text);
    }

    .folder-path {
        color: var(--color-text-secondary);
        margin: 0 0 var(--spacing-lg) 0;
        word-break: break-all;
    }

    .detail {
        margin: 0;
    }

    /* Wraps rather than overflows: a localized "Go back" plus its chip can outgrow one line. */
    .actions {
        display: flex;
        flex-wrap: wrap;
        justify-content: center;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-lg);
    }
</style>
