<script lang="ts">
    /**
     * The once-ever toast the first quick filter raises (`quick-filter-intro.ts`):
     * what happened, how to see every file again, and the two ways to choose
     * otherwise. The frame's own × dismisses it, so the third button is "Got it".
     */
    import Button from '$lib/ui/Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { dismissToast } from '$lib/ui/toast'
    import { QUICK_FILTER_INTRO_TOAST_ID, openTypingModeSettings, switchToJumpMode } from './quick-filter-intro-actions'
</script>

<div class="content">
    <strong class="title">{tString('fileExplorer.quickFilter.intro.title')}</strong>
    <span class="body">{tString('fileExplorer.quickFilter.intro.body')}</span>
    <span class="body"><strong>{tString('fileExplorer.quickFilter.intro.escape')}</strong></span>
    <span class="body">{tString('fileExplorer.quickFilter.intro.jumpNote')}</span>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={switchToJumpMode}>
            {tString('fileExplorer.quickFilter.intro.switchToJump')}
        </Button>
        <Button
            size="mini"
            variant="secondary"
            onclick={() => {
                void openTypingModeSettings()
            }}
        >
            {tString('fileExplorer.quickFilter.intro.openSettings')}
        </Button>
        <Button
            size="mini"
            variant="primary"
            onclick={() => {
                dismissToast(QUICK_FILTER_INTRO_TOAST_ID)
            }}
        >
            {tString('fileExplorer.quickFilter.intro.gotIt')}
        </Button>
    </div>
</div>

<style>
    /* Block flow, so the text wraps around the toast's corner float (`ui/DETAILS.md` § Toast system). */
    .content {
        font-size: var(--font-size-sm);
    }

    .title,
    .body {
        display: block;
        color: var(--color-text-primary);
    }

    .body {
        margin-top: var(--spacing-xxs);
    }

    .title {
        font-weight: 600;
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
