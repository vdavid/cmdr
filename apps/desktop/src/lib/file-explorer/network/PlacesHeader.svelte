<script lang="ts">
    /**
     * A host's share list header: Back, the host's name and the account the list is
     * signed in as ("as guest"), "Sign in as…", "Forget saved password" when one is
     * known to be stored, and the share count. What each button does is
     * `PlacesBrowser.svelte`'s.
     */
    import Button from '#lib/ui/Button.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import { signedInAsLabel } from './signed-in-as-label'
    import type { SignedInAs } from './signed-in-as'

    interface Props {
        hostLabel: string
        /** The account the listing signed in as, `undefined` when no listing said. */
        account: SignedInAs | undefined
        /** Whether a stored password is known (from what this session already read). */
        canForgetPassword: boolean
        shareCount: number
        onBack?: () => void
        onSignInAs: () => void
        /** "Browse as guest", when there's a guest to go back to; absent, no button. */
        onUseGuest?: () => void
        onForgetPassword: () => void
    }

    const {
        hostLabel,
        account,
        canForgetPassword,
        shareCount,
        onBack,
        onSignInAs,
        onUseGuest,
        onForgetPassword,
    }: Props = $props()
</script>

<div class="header-row">
    <Button variant="secondary" size="mini" onclick={onBack}>
        <span class="btn-icon-label">
            <Icon name="arrow-left" size={14} aria-hidden="true" />
            {tString('fileExplorer.network.share.backArrow')}
        </span>
    </Button>
    <span class="host-name">{hostLabel}</span>
    {#if account}<span class="account">{signedInAsLabel(account)}</span>{/if}
    <button class="header-action" onclick={onSignInAs}>
        <Icon name="user" size={12} aria-hidden="true" />
        {tString('fileExplorer.network.share.signInAs')}
    </button>
    {#if onUseGuest}
        <button class="header-action" onclick={onUseGuest}>{tString('fileExplorer.network.share.useGuest')}</button>
    {/if}
    {#if canForgetPassword}
        <button
            class="header-action"
            onclick={onForgetPassword}
            use:tooltip={tString('fileExplorer.network.share.forgetPasswordTooltip')}
        >
            <Icon name="key" size={12} aria-hidden="true" />
            {tString('fileExplorer.network.share.forgetPassword')}
        </button>
    {/if}
    <span class="share-count"
        >{tString('fileExplorer.network.share.shareCount', {
            count: shareCount,
            countText: formatInteger(shareCount),
        })}</span
    >
</div>

<style>
    .header-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-md);
        padding: var(--spacing-sm) var(--spacing-md);
        background-color: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border-strong);
    }

    .btn-icon-label {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .host-name {
        font-weight: 500;
        color: var(--color-text-primary);
        white-space: nowrap;
    }

    /* Reads as one phrase with the name before it ("Testnas as guest"), so it sits closer. */
    .account {
        margin-left: calc(var(--spacing-md) * -1 + var(--spacing-xs));
        color: var(--color-text-tertiary);
        white-space: nowrap;
    }

    .header-action {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: 1px var(--spacing-sm);
        font-family: var(--font-system), sans-serif;
        font-size: calc(var(--font-size-sm) * 0.9);
        color: var(--color-text-tertiary);
        background: none;
        border: 1px solid transparent;
        border-radius: var(--radius-sm);
    }

    .header-action:hover {
        color: var(--color-text-secondary);
        border-color: var(--color-border);
        background-color: var(--color-bg-tertiary);
    }

    .share-count {
        color: var(--color-text-tertiary);
        margin-left: auto;
        white-space: nowrap;
    }
</style>
