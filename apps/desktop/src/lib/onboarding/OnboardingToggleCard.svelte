<script lang="ts">
    /**
     * One bordered toggle card of the onboarding sheet: a title and description on the
     * left, a registry-backed `<SettingSwitch>` with a caption under it on the right.
     * `StepBeta` (the analytics opt-out) and `StepOptional` (the four optional setup
     * toggles) both render it, so the two steps stay pixel-identical.
     *
     * The description is a snippet, not a string: `StepOptional` puts `Trans` markup and a
     * benefits list there, styled by the parent's own `.toggle-desc` / `.toggle-list`.
     */
    import type { Snippet } from 'svelte'
    import SettingSwitch from '$lib/settings/components/SettingSwitch.svelte'
    import type { SettingId } from '$lib/settings'

    interface Props {
        /** Id for the `<h3>`, which labels the card's `<section>`. */
        titleId: string
        title: string
        /** The setting the switch reads and writes. */
        settingId: SettingId
        /** The small note under the switch. */
        caption: string
        /** The description under the title. */
        children: Snippet
    }

    const { titleId, title, settingId, caption, children }: Props = $props()
</script>

<section class="toggle-block" aria-labelledby={titleId}>
    <header class="toggle-header">
        <div class="toggle-text">
            <h3 id={titleId} class="toggle-title">{title}</h3>
            {@render children()}
        </div>
        <div class="toggle-control">
            <SettingSwitch id={settingId} />
            <p class="toggle-caption">{caption}</p>
        </div>
    </header>
</section>

<style>
    .toggle-block {
        margin-bottom: var(--spacing-md);
        padding: var(--spacing-lg);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        background: var(--color-bg-primary);
    }

    .toggle-block:last-child {
        margin-bottom: 0;
    }

    .toggle-header {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-lg);
    }

    .toggle-text {
        flex: 1;
        min-width: 0;
    }

    .toggle-control {
        flex-shrink: 0;
        display: flex;
        flex-direction: column;
        align-items: flex-end;
        gap: var(--spacing-xs);
        padding-top: var(--spacing-xxs);
    }

    .toggle-caption {
        margin: 0;
        max-width: 14rem;
        text-align: right;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .toggle-title {
        margin: 0 0 var(--spacing-xs);
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }
</style>
