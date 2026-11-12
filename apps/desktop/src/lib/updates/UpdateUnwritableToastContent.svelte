<script lang="ts">
    import type { Snippet } from 'svelte'
    import LinkButton from '#lib/ui/LinkButton.svelte'
    import Trans from '#lib/intl/Trans.svelte'
    import { pathParam, tString } from '#lib/intl/messages.svelte.js'
    import { openExternalUrl } from '#lib/tauri-commands/index.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import type { PermissionBlocker } from './update-state.svelte'

    interface Props {
        /** The release that's out. */
        version: string
        /** Whether this user can replace the app themselves (an administrator) or has to ask. */
        blocker: PermissionBlocker
        /** The running bundle, so the copy names the real folder and app name. */
        bundlePath: string
    }

    const { version, blocker, bundlePath }: Props = $props()

    const log = getAppLogger('updater')
    const websiteUrl = 'https://getcmdr.com'

    async function handleOpenWebsite() {
        try {
            await openExternalUrl(websiteUrl)
        } catch (e) {
            log.warn("Couldn't open getcmdr.com from the unwritable-bundle notice: {error}", { error: e })
        }
    }
</script>

{#snippet websiteLink(children: Snippet)}
    <LinkButton
        href={websiteUrl}
        onclick={(e: MouseEvent) => {
            e.preventDefault()
            void handleOpenWebsite()
        }}>{@render children()}</LinkButton
    >
{/snippet}

<div>
    {#if blocker === 'notWritableAdminUser'}
        <span class="unwritable-headline">{tString('updates.unwritable.adminHeadline', { version })}</span>
        <span class="unwritable-detail">
            <Trans key="updates.unwritable.adminDetail" params={{ path: pathParam(bundlePath) }} snippets={{ website: websiteLink }} />
        </span>
    {:else}
        <span class="unwritable-headline">{tString('updates.unwritable.standardHeadline', { version })}</span>
        <span class="unwritable-detail">{tString('updates.unwritable.standardDetail')}</span>
    {/if}
</div>

<style>
    .unwritable-headline {
        font-size: var(--font-size-sm);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .unwritable-detail {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        /* A bundle path is one long unbreakable token; let it wrap rather than widen the toast. */
        overflow-wrap: anywhere;
    }
</style>
