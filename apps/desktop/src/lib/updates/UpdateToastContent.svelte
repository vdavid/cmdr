<script lang="ts">
    import { dismissToast } from '#lib/ui/toast/index.js'
    import Button from '#lib/ui/Button.svelte'
    import ToastActions from '#lib/ui/toast/ToastActions.svelte'
    import LinkButton from '#lib/ui/LinkButton.svelte'
    import { openExternalUrl, relaunchApp } from '#lib/tauri-commands/index.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { updateState } from './update-state.svelte'

    const log = getAppLogger('updater')

    /**
     * What the staged update fixes, security-wise. Set, the toast leads with that and says what to
     * do; `null`, it reads exactly as it always has. The link is already vetted backend-side against
     * the advisory allowlist (`src-tauri/src/security_advisories/`), so `url` is `null` rather than
     * anything else when the list named a link it refused.
     */
    const notice = $derived(updateState.securityNotice)

    /**
     * Both ends of the version row, or `null` when either is unknown. A toast raised from `ready`
     * always has both (`previousVersion` is set when a check starts, `nextVersion` when an update
     * is found), so this is a guard rather than a real branch: it keeps a half-populated state from
     * rendering `v → v0.29.0` instead of dropping the row.
     */
    const versions = $derived(
        updateState.previousVersion !== null && updateState.nextVersion !== null
            ? { prev: updateState.previousVersion, next: updateState.nextVersion }
            : null,
    )

    function handleRestart() {
        void relaunchApp()
    }

    function handleDismiss() {
        dismissToast('update')
    }

    async function handleOpenAdvisory(url: string) {
        try {
            await openExternalUrl(url)
        } catch (e) {
            log.warn("Couldn't open the security advisory link: {error}", { error: e })
        }
    }
</script>

<div>
    {#if notice !== null}
        <span class="update-headline">{tString('updates.toast.securityFix', { count: notice.count })}</span>
        <span class="update-detail">{tString('updates.toast.securityFixDetail')}</span>
        {#if notice.url !== null}
            {@const url = notice.url}
            <span class="update-advisory">
                <LinkButton
                    href={url}
                    onclick={(e: MouseEvent) => {
                        e.preventDefault()
                        void handleOpenAdvisory(url)
                    }}>{tString('updates.toast.readAdvisory')}</LinkButton
                >
            </span>
        {/if}
    {:else}
        <span class="update-headline">{tString('updates.toast.ready')}</span>
        <span class="update-detail">{tString('updates.toast.readyDetail')}</span>
    {/if}
    {#if versions !== null}
        <!-- `role="img"` so the arrow is announced as the sentence in `versionChangeAria` rather
             than read out as a bare symbol, in whatever language the screen reader names it. -->
        <span class="update-versions" role="img" aria-label={tString('updates.toast.versionChangeAria', versions)}>
            {tString('updates.toast.versionChange', versions)}
        </span>
    {/if}
    <ToastActions>
        <Button variant="secondary" size="mini" onclick={handleDismiss}>{tString('updates.toast.later')}</Button>
        <Button variant="primary" size="mini" onclick={handleRestart}>{tString('updates.toast.restart')}</Button>
    </ToastActions>
</div>

<style>
    .update-headline {
        font-size: var(--font-size-sm);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .update-detail {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .update-advisory {
        font-size: var(--font-size-sm);
    }

    .update-versions {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        /* Version numbers line up under each other across re-renders instead of jittering. */
        font-variant-numeric: tabular-nums;
    }
</style>
