<!--
  The "Allow cloud AI" switch and its disclosure: what every AI feature sends once it's on.
  Shared by Settings > AI > Provider and the onboarding AI step, and rendered only while the
  AI mode is Cloud (Local sends nothing off the Mac, so it needs no consent).

  ❌ This is the ONLY place that may grant cloud consent: the switch's own click calls
  `acceptCloudConsent`, and `cloud-consent-call-sites.test.ts` fails if anything else imports
  it. Switching off goes through `declineCloudConsent`, which also stops in-flight cloud calls
  and holds a "no" the store refused. `ai.provider` is never touched here.

  The copy is human-reviewed (principle 4) and versioned: a material change to the
  `ai.cloudConsent.*` text needs a `CLOUD_AI_CONSENT_VERSION` bump in the backend
  (`src-tauri/src/ai/cloud_consent.rs`).
-->
<script lang="ts">
    import type { Snippet } from 'svelte'
    import Switch from '$lib/ui/Switch.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { managedAiRefusalMessage } from '$lib/managed-policy/ai-refusal'
    import {
        acceptCloudConsent,
        cloudConsentState,
        declineCloudConsent,
        refreshCloudConsent,
        CLOUD_CONSENT_ANCHOR,
    } from './cloud-consent.svelte'

    interface Props {
        /** Stamps the DOM id deep links scroll to. Off in onboarding, which has no deep links. */
        anchor?: boolean
    }

    const { anchor = false }: Props = $props()

    $effect(() => {
        void refreshCloudConsent()
    })

    const accepted = $derived(cloudConsentState.accepted === true)
    // The organization rules out every cloud host: the switch is locked off, with the reason in view.
    const managed = $derived(cloudConsentState.managed)
    const managedNoteId = $derived(anchor ? 'settings-ai-cloud-consent-managed' : 'onboarding-ai-cloud-consent-managed')
    // Follows the store; the switch writes it on a click. The handler below resyncs after
    // every attempt: a refused write leaves `accepted` unchanged, so the derivation wouldn't
    // re-run to flip the switch back.
    let checked = $derived(accepted)

    // The disclosure's fold follows the switch: open while it's off (people read it while
    // deciding), folded once it's on, so what sits below it (onboarding's service setup) stays
    // in view. The user's own open/close writes through `bind:open` until the switch moves.
    let disclosureOpen = $derived(!accepted)

    let busy = $state(false)
    /** The last flip didn't reach the store; the switch shows what did. */
    let notSaved = $state(false)

    async function onCheckedChange(next: boolean): Promise<void> {
        if (busy) return
        busy = true
        notSaved = false
        try {
            const outcome = next ? await acceptCloudConsent() : await declineCloudConsent()
            notSaved = outcome === 'notSaved'
        } finally {
            checked = cloudConsentState.accepted === true
            disclosureOpen = !checked
            busy = false
        }
    }

    // Local ISO date (YYYY-MM-DD) for the "on since" line, style-preferred and locale-safe.
    function localIsoDate(unixSecs: number): string {
        const d = new Date(unixSecs * 1000)
        const pad = (n: number): string => String(n).padStart(2, '0')
        return `${String(d.getFullYear())}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
    }
</script>

{#snippet b(children: Snippet)}<strong>{@render children()}</strong>{/snippet}

<div class="cloud-consent" id={anchor ? CLOUD_CONSENT_ANCHOR : undefined}>
    <div class="consent-row">
        <div class="consent-text">
            <span class="consent-label">{tString('ai.cloudConsent.label')}</span>
            <span class="consent-description">{tString('ai.cloudConsent.description')}</span>
            {#if accepted && cloudConsentState.acceptedAt}
                <span class="consent-since">
                    {tString('settings.ai.cloudConsent.onSince', { date: localIsoDate(cloudConsentState.acceptedAt) })}
                </span>
            {/if}
        </div>
        <Switch
            bind:checked
            disabled={busy || cloudConsentState.accepted === null || managed !== null}
            ariaLabel={tString('ai.cloudConsent.label')}
            ariaDescribedBy={managed ? managedNoteId : undefined}
            onCheckedChange={(next: boolean) => void onCheckedChange(next)}
            data-test="cloud-ai-consent"
        />
    </div>
    {#if managed}
        <p class="consent-managed" id={managedNoteId}>{managedAiRefusalMessage(managed)}</p>
    {/if}
    {#if notSaved}
        <p class="consent-not-saved" role="status">{tString('ai.cloudConsent.notSaved')}</p>
    {/if}

    <!-- Open while off: this is what the person is agreeing to, so it's in view before the
         click. Turning it on folds it away; it stays one click from reach. -->
    <details class="disclosure" bind:open={disclosureOpen}>
        <summary>{tString('ai.cloudConsent.disclosureTitle')}</summary>
        <div class="disclosure-body">
            <p>{tString('ai.cloudConsent.intro')}</p>
            <p>{tString('ai.cloudConsent.whereItGoes')}</p>
            <ul>
                <li><Trans key="ai.cloudConsent.folderSuggestions" snippets={{ b }} /></li>
                <li><Trans key="ai.cloudConsent.search" snippets={{ b }} /></li>
                <li><Trans key="ai.cloudConsent.selection" snippets={{ b }} /></li>
                <li>
                    <Trans key="ai.cloudConsent.askCmdr.title" snippets={{ b }} />
                    <ul>
                        <li>{tString('ai.cloudConsent.askCmdr.item.messages')}</li>
                        <li>{tString('ai.cloudConsent.askCmdr.item.names')}</li>
                        <li>{tString('ai.cloudConsent.askCmdr.item.sizes')}</li>
                        <li>{tString('ai.cloudConsent.askCmdr.item.contents')}</li>
                        <li>{tString('ai.cloudConsent.askCmdr.item.envelope')}</li>
                        <li>{tString('ai.cloudConsent.askCmdr.item.attachments')}</li>
                        <li>{tString('ai.cloudConsent.askCmdr.item.memory')}</li>
                    </ul>
                    <p>{tString('ai.cloudConsent.askCmdr.contentsRule')}</p>
                    <p>{tString('ai.cloudConsent.askCmdr.memory')}</p>
                    <p>{tString('ai.cloudConsent.askCmdr.proactive')}</p>
                    <p>{tString('ai.cloudConsent.askCmdr.chatsStayLocal')}</p>
                </li>
            </ul>
            <p class="fine">{tString('ai.cloudConsent.logsNote')}</p>
            <p>{tString('ai.cloudConsent.turnOffAnyTime')}</p>
        </div>
    </details>
</div>

<style>
    .cloud-consent {
        padding: var(--spacing-sm) 0;
    }

    .consent-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-md);
    }

    .consent-text {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        min-width: 0;
    }

    .consent-label {
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .consent-description {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .consent-since {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .consent-managed {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .consent-not-saved {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-warning-text);
    }

    .disclosure {
        margin-top: var(--spacing-sm);
        font-size: var(--font-size-sm);
        line-height: var(--font-line-height-prose);
        color: var(--color-text-secondary);
    }

    .disclosure summary {
        cursor: default;
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .disclosure-body {
        margin-top: var(--spacing-sm);
    }

    .disclosure-body ul {
        margin: 0 0 var(--spacing-sm);
        padding-left: var(--spacing-lg);
    }

    .disclosure-body li {
        margin-bottom: var(--spacing-xxs);
    }

    .disclosure-body li ul {
        margin-top: var(--spacing-xxs);
    }

    .disclosure-body p {
        margin: 0 0 var(--spacing-sm);
    }

    .disclosure-body strong {
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .fine {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }
</style>
