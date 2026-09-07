<script lang="ts">
    import Icon from '$lib/ui/Icon.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { getSetting, setSetting } from '$lib/settings'
    import { getVolumes } from '$lib/stores/volume-store.svelte'
    import { openExternalUrl } from '$lib/tauri-commands'
    import { isMtpVolumeId } from '$lib/mtp'
    import { getAppLogger } from '$lib/logging/logger'
    import { shouldShowAdbHint } from './should-show-adb-hint'

    /**
     * One quiet line at the top of a pane showing a phone over MTP, offering the
     * fuller way into the same device.
     *
     * ❗ **Discoverability is the whole point.** A phone with USB debugging off
     * is a plain MTP row and says nothing about ADB, so without this line nobody
     * finds the feature at all (`DETAILS.md` § "The line offering USB
     * debugging").
     *
     * ❗ **It goes away for good on the ×** (`behavior.adbHintDismissed`), and it
     * never appears when the same phone ALREADY has an ADB row: the decision is
     * `should-show-adb-hint.ts`, where it can be read at a glance.
     *
     * ❌ Not a toast and not a dialog: it is an offer, not news, so it waits in
     * the pane for someone to notice it rather than interrupting.
     */
    interface Props {
        /** The pane's volume id. */
        volumeId: string
    }

    const { volumeId }: Props = $props()

    const log = getAppLogger('adb')

    /**
     * Android's own instructions, ❌ not a page of ours: turning on developer
     * options is a vendor procedure that changes with each Android release, and
     * Google ships it translated into more languages than Cmdr has.
     */
    const USB_DEBUGGING_HELP_URL = 'https://developer.android.com/studio/debug/dev-options'

    /** Flipped by the ×, so the line goes at the click rather than at the next read. */
    let dismissedNow = $state(false)

    const volumes = $derived(getVolumes())
    const deviceName = $derived(volumes.find((v) => v.id === volumeId)?.name ?? '')

    const visible = $derived(
        shouldShowAdbHint({
            isMtpPane: isMtpVolumeId(volumeId),
            deviceName,
            deviceNames: volumes.filter((v) => v.category === 'mobile_device').map((v) => v.name),
            adbEnabled: getSetting('fileOperations.adbEnabled'),
            dismissed: dismissedNow || getSetting('behavior.adbHintDismissed'),
        }),
    )

    function dismiss() {
        // Flipped locally first, so the line goes at the click rather than at
        // whenever the store notices.
        dismissedNow = true
        try {
            setSetting('behavior.adbHintDismissed', true)
        } catch (e: unknown) {
            // It is already gone for this session and would come back next
            // launch. Not worth a word to the user.
            log.warn('Remembering the dismissed USB-debugging hint broke down: {error}', { error: String(e) })
        }
    }
</script>

{#if visible}
    <div class="adb-hint">
        <span class="hint-icon"><Icon name="info" size={13} aria-hidden="true" /></span>
        <span class="hint-text">{tString('adb.hint.text')}</span>
        <LinkButton
            href={USB_DEBUGGING_HELP_URL}
            onclick={(e: MouseEvent) => { e.preventDefault(); void openExternalUrl(USB_DEBUGGING_HELP_URL) }}
        >
            {tString('adb.hint.how')}
        </LinkButton>
        <button
            type="button"
            class="hint-dismiss"
            aria-label={tString('adb.hint.dismiss')}
            onclick={dismiss}
        >
            <Icon name="x" size={12} aria-hidden="true" />
        </button>
    </div>
{/if}

<style>
    /* Quiet by design: it sits between the breadcrumb and the listing, at the
       weight of a secondary label, so it reads as an offer rather than as a
       warning about the pane below it. */
    .adb-hint {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xs) var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        background-color: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border);
    }

    .hint-icon {
        display: inline-flex;
        align-items: center;
        color: var(--color-text-tertiary);
    }

    .hint-text {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .hint-dismiss:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }

    .hint-dismiss {
        display: inline-flex;
        align-items: center;
        margin-left: auto;
        padding: 0;
        background: none;
        border: none;
        color: var(--color-text-tertiary);
    }

    .hint-dismiss:hover {
        color: var(--color-text-primary);
    }
</style>
