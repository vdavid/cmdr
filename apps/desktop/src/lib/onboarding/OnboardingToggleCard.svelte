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
    import Icon from '$lib/ui/Icon.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
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
        /**
         * The long version, behind an info glyph beside the title. Rendered into a hidden
         * host and handed to the tooltip as a live `contentEl`, so it can carry real
         * paragraphs and lists instead of one run-on line. Omit for a card whose
         * description already says everything.
         */
        details?: Snippet
        /** Accessible name for the info glyph. Required whenever `details` is passed. */
        detailsLabel?: string
    }

    const { titleId, title, settingId, caption, children, details, detailsLabel }: Props = $props()

    /**
     * The tooltip adopts THIS element, never the `hidden` wrapper around it: an adopted
     * element keeps its own attributes, so handing over the hidden host would show an
     * empty tooltip.
     */
    let detailsEl = $state<HTMLDivElement>()
</script>

<section class="toggle-block" aria-labelledby={titleId}>
    <header class="toggle-header">
        <div class="toggle-text">
            <h3 id={titleId} class="toggle-title">
                {title}
                {#if details}
                    <!-- A `<button>`, so the details are one Tab away too: the tooltip action
                         opens on focus as well as hover, and a native `title` would not. -->
                    <button
                        type="button"
                        class="toggle-info"
                        aria-label={detailsLabel}
                        use:tooltip={{ contentEl: detailsEl }}
                    >
                        <Icon name="info" size={14} aria-hidden="true" />
                    </button>
                {/if}
            </h3>
            {@render children()}
        </div>
        <div class="toggle-control">
            <SettingSwitch id={settingId} />
            <p class="toggle-caption">{caption}</p>
        </div>
    </header>
    {#if details}
        <!-- Parked outside `.toggle-text` on purpose: a hidden sibling in there would
             still count for `:last-child`, and the summary above it would keep a
             paragraph gap under it with nothing to separate. -->
        <div hidden>
            <div bind:this={detailsEl} class="toggle-details">{@render details()}</div>
        </div>
    {/if}
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
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        margin: 0 0 var(--spacing-xs);
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    /* The tooltip's own box is narrow, so the details column just fills it; the paragraph
       and list rhythm inside comes from the step that owns the snippet. */
    .toggle-details {
        display: block;
    }

    .toggle-info {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        padding: 0;
        border: none;
        background: transparent;
        color: var(--color-text-tertiary);
        transition: color var(--transition-base);
    }

    .toggle-info:hover {
        color: var(--color-text-primary);
    }

    .toggle-info:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
        border-radius: var(--radius-xs);
    }
</style>
