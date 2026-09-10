<script lang="ts">
    /**
     * The once-ever INFO notice raised the first time another app's "Show in
     * Finder" actually lands in a Cmdr pane.
     *
     * ❗ It offers a way INTO Settings and ❌ never a "Turn it off" button:
     * switching this back should cost one more click, in the place the setting
     * lives, so nobody undoes their own choice by reflex from a toast.
     * `reveal-activation-notice.ts` decides whether it appears at all.
     */
    import Button from '$lib/ui/Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { openSettingsToRevealHandler } from './reveal-settings-link'
</script>

<div class="content">
    <strong class="title">{tString('main.revealActivation.title')}</strong>
    <span class="body">{tString('main.revealActivation.body')}</span>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={() => void openSettingsToRevealHandler()}>
            {tString('main.revealActivation.openSettings')}
        </Button>
    </div>
</div>

<style>
    .content {
        font-size: var(--font-size-sm);
    }

    .title {
        color: var(--color-text-primary);
        font-weight: 600;
    }

    .body {
        color: var(--color-text-primary);
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        margin-top: calc(var(--spacing-xs) + var(--spacing-xxs));
    }
</style>
