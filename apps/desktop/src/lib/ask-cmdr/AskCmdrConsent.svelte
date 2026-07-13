<!--
  The Ask Cmdr opt-in screen: shown in the rail before the user has accepted the current
  consent copy (spec §2.1 privacy line; plan §12). Nothing is sent to a provider until the
  user turns Ask Cmdr on here. "Not now" closes the rail. The exact copy is human-reviewed
  (principle 6); it lives in the catalog (`askCmdr.consent.*`), shared with the settings
  section's "what Ask Cmdr sends" disclosure.
-->
<script lang="ts">
    import Icon from '$lib/ui/Icon.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { acceptConsent } from './ask-cmdr-consent.svelte'
    import { closeRail, openRail } from './ask-cmdr-trigger.svelte'

    let acceptButton = $state<HTMLButtonElement | null>(null)
    let accepting = $state(false)

    // Focus the primary action on mount so the keyboard user lands on it (the rail mounts
    // this only when consent is needed, so the composer's focus effect doesn't run).
    $effect(() => {
        acceptButton?.focus()
    })

    async function onAccept(): Promise<void> {
        if (accepting) return
        accepting = true
        try {
            // Re-run open once consent lands so the rail bootstraps any existing thread
            // (a re-accept after a prior turn-off) and focuses the composer.
            if (await acceptConsent()) await openRail()
        } finally {
            accepting = false
        }
    }
</script>

<div class="consent" role="group" aria-labelledby="ask-cmdr-consent-title">
    <div class="consent-scroll">
        <span class="consent-glyph"><Icon name="sparkles" size={28} aria-hidden="true" /></span>
        <h2 id="ask-cmdr-consent-title" class="consent-title">{tString('askCmdr.consent.title')}</h2>
        <p class="consent-intro">{tString('askCmdr.consent.intro')}</p>
        <ul class="consent-list">
            <li>{tString('askCmdr.consent.item.messages')}</li>
            <li>{tString('askCmdr.consent.item.names')}</li>
            <li>{tString('askCmdr.consent.item.sizes')}</li>
            <li>{tString('askCmdr.consent.item.envelope')}</li>
            <li>{tString('askCmdr.consent.item.attachments')}</li>
        </ul>
        <p class="consent-para">{tString('askCmdr.consent.noContents')}</p>
        <p class="consent-para">{tString('askCmdr.consent.local')}</p>
        <p class="consent-note">{tString('askCmdr.consent.logsNote')}</p>
    </div>
    <div class="consent-actions">
        <button type="button" class="consent-decline" onclick={closeRail}>
            {tString('askCmdr.consent.decline')}
        </button>
        <button
            type="button"
            class="consent-accept"
            bind:this={acceptButton}
            disabled={accepting}
            onclick={() => void onAccept()}
        >
            {tString('askCmdr.consent.accept')}
        </button>
    </div>
</div>

<style>
    .consent {
        display: flex;
        flex-direction: column;
        flex: 1;
        min-height: 0;
    }

    .consent-scroll {
        flex: 1;
        min-height: 0;
        overflow-y: auto;
        padding: var(--spacing-md);
    }

    .consent-glyph {
        display: block;
        color: var(--color-accent-text);
    }

    .consent-title {
        margin: var(--spacing-sm) 0 var(--spacing-md);
        font-size: var(--font-size-lg);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .consent-intro {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-sm);
        line-height: 1.5;
        color: var(--color-text-primary);
    }

    .consent-list {
        margin: 0 0 var(--spacing-md);
        padding-left: var(--spacing-lg);
        font-size: var(--font-size-sm);
        line-height: 1.5;
        color: var(--color-text-secondary);
    }

    .consent-list li {
        margin-bottom: var(--spacing-xxs);
    }

    .consent-para {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-sm);
        line-height: 1.5;
        color: var(--color-text-secondary);
    }

    .consent-note {
        margin: var(--spacing-sm) 0 0;
        font-size: var(--font-size-xs);
        line-height: 1.5;
        color: var(--color-text-tertiary);
    }

    .consent-actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        padding: var(--spacing-sm) var(--spacing-md);
        border-top: 1px solid var(--color-border-subtle);
    }

    .consent-accept,
    .consent-decline {
        padding: var(--spacing-xs) var(--spacing-md);
        font: inherit;
        font-size: var(--font-size-sm);
        font-weight: 500;
        border-radius: var(--radius-sm);
        border: 1px solid var(--color-border);
    }

    .consent-accept {
        color: var(--color-accent-fg);
        background: var(--color-accent);
        border-color: var(--color-accent);
    }

    .consent-accept:hover:not(:disabled) {
        background: var(--color-accent-hover);
    }

    .consent-accept:disabled {
        opacity: 0.6;
    }

    .consent-decline {
        color: var(--color-text-primary);
        background: var(--color-bg-tertiary);
    }

    .consent-decline:hover {
        background: var(--color-bg-secondary);
    }
</style>
