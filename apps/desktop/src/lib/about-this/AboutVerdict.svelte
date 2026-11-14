<!--
  The card's AI take, in the slot right above What removing it means: a labeled block, set apart from the fact rows by
  its surface, holding two or three sentences a model wrote from the card's facts. Or, when the card gets none, one
  quiet line saying why (cloud AI needs allowing again, no key, the organization's policy, too many in a minute).

  Every model writes; Rust sends how far it can be trusted with `verdictPlanned`, and below `trusted` the block carries
  a warning line from its first frame: red with an alert icon for a model known to be often wrong (with a link to
  change the model), yellow with an info icon for one nobody tested. Icon and words, so color is never the only signal.

  ❗ The text is untrusted (a hostile file name can steer it): it renders as ONE text node, ❌ never Markdown, `{@html}`,
  or a link. Nothing else on the card reads it.

  ❗ Nothing moves: the block reserves three lines for the text and one for the line saying what it was given, from its
  first frame. The text may grow past three lines while it streams, downward only. Its end marks ("Stopped", a
  provider's answer) sit inline after the text. Nothing animates.
-->
<script lang="ts">
    import { tString } from '#lib/intl/messages.svelte.js'
    import Button from '#lib/ui/Button.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import LinkButton from '#lib/ui/LinkButton.svelte'
    import { openCloudConsentSettings } from '#lib/ai/cloud-consent.svelte.js'
    import { openSettingsWindow } from '#lib/settings/settings-window.js'
    import { managedAiRefusalMessage } from '#lib/managed-policy/ai-refusal.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { verdictPhase, verdictProblem, type AboutVerdict } from './about-verdict'
    import { VERDICT_GIVEN, VERDICT_PROBLEM } from './about-words'

    interface Props {
        verdict: AboutVerdict
        /** The Stop button: stops the verdict alone; the card stays. */
        onStop: () => void
    }

    const { verdict, onStop }: Props = $props()

    const log = getAppLogger('aboutThis')

    function openProviderSettings(): void {
        openSettingsWindow('about-this-card', ['AI', 'Provider']).catch((e: unknown) => {
            log.warn('opening the AI settings failed: {error}', { error: e })
        })
    }
</script>

{#if verdict.status === 'planned'}
    {@const phase = verdictPhase(verdict)}
    {@const problem = verdictProblem(verdict)}
    <section class="verdict" aria-labelledby="about-verdict-label" data-verdict={phase}>
        <div class="head">
            <span class="glyph" aria-hidden="true"><Icon name="sparkles" size={14} /></span>
            <h3 id="about-verdict-label">{tString('aboutThis.verdict.label')}</h3>
            {#if phase === 'fromEarlier'}
                <span class="tag">{tString('aboutThis.verdict.fromEarlier')}</span>
            {/if}
            {#if phase === 'waiting' || phase === 'streaming'}
                <span class="stop">
                    <Button
                        variant="secondary"
                        size="mini"
                        tooltipContent={tString('aboutThis.verdict.stopTooltip')}
                        onclick={onStop}>{tString('aboutThis.verdict.stop')}</Button
                    >
                </span>
            {/if}
        </div>
        {#if verdict.trust === 'oftenWrong'}
            <p class="trust often-wrong" data-verdict-trust="oftenWrong">
                <span class="trust-glyph" aria-hidden="true"><Icon name="triangle-alert" size={14} /></span>
                <span>
                    {tString('aboutThis.verdict.trust.oftenWrong')}
                    <LinkButton onclick={openProviderSettings}>{tString('aboutThis.verdict.trust.changeModel')}</LinkButton>
                </span>
            </p>
        {:else if verdict.trust === 'untested'}
            <p class="trust untested" data-verdict-trust="untested">
                <span class="trust-glyph" aria-hidden="true"><Icon name="circle-alert" size={14} /></span>
                <span>{tString('aboutThis.verdict.trust.untested')}</span>
            </p>
        {/if}
        <p class="given">{verdict.given === null ? '' : tString(VERDICT_GIVEN[verdict.given])}</p>
        <!-- One line on purpose: under `pre-wrap`, any whitespace between these tags would show. -->
        <p class="text">{#if phase === 'waiting'}<span class="quiet">{tString('aboutThis.verdict.writing')}</span>{:else if verdict.text !== ''}<span data-verdict-text>{verdict.text}</span>{/if}{#if phase === 'stopped'}<span class="mark">{tString('aboutThis.verdict.stopped')}</span>{:else if problem !== null}<span class="mark">{tString(VERDICT_PROBLEM[problem])}</span>{/if}</p>
    </section>
{:else if verdict.why.kind === 'noCloudConsent'}
    <p class="unavailable" data-verdict="noCloudConsent">
        {tString('aboutThis.verdict.unavailable.noCloudConsent')}
        <LinkButton
            onclick={() => {
                openCloudConsentSettings('about-this-card')
            }}>{tString('aboutThis.verdict.unavailable.openAiSettings')}</LinkButton
        >
    </p>
{:else if verdict.why.kind === 'notConfigured'}
    <p class="unavailable" data-verdict="notConfigured">
        {tString('aboutThis.verdict.unavailable.notConfigured')}
        <LinkButton onclick={openProviderSettings}>{tString('aboutThis.verdict.unavailable.openAiSettings')}</LinkButton>
    </p>
{:else if verdict.why.kind === 'notReady'}
    <p class="unavailable" data-verdict="notReady">{tString('aboutThis.verdict.unavailable.notReady')}</p>
{:else if verdict.why.kind === 'managed'}
    <p class="unavailable" data-verdict="managed">{managedAiRefusalMessage(verdict.why.refusal)}</p>
{:else if verdict.why.kind === 'tooManyRequests'}
    <p class="unavailable" data-verdict="tooManyRequests">{tString('aboutThis.verdict.unavailable.tooManyRequests')}</p>
{/if}

<style>
    /* Its own surface, never a row's look: a model's sentence must not pass for one of Cmdr's facts. */
    .verdict {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        padding: var(--spacing-sm) var(--spacing-md);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        background: var(--color-bg-secondary);
    }

    .head {
        display: flex;
        gap: var(--spacing-xs);
        align-items: center;
        /* The Stop button's height, held after it goes, so the text below never moves up. */
        min-height: var(--spacing-xl);
    }

    .glyph {
        display: inline-flex;
        color: var(--color-accent-text);
    }

    h3 {
        margin: 0;
        font-size: var(--font-size-sm);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .tag {
        color: var(--color-text-tertiary);
    }

    .stop {
        margin-left: auto;
    }

    /* A model's trust, in the warning roles' text tokens (AA on this surface in both themes). */
    .trust {
        display: flex;
        gap: var(--spacing-xs);
        align-items: flex-start;
        margin: 0;
        font-size: var(--font-size-xs);
    }

    .trust-glyph {
        display: inline-flex;
        /* Centers the 14px glyph on the first line of text. */
        padding-top: 1px;
    }

    .often-wrong {
        color: var(--color-error-text);
    }

    .untested {
        color: var(--color-warning-text);
    }

    .given {
        margin: 0;
        min-height: calc(var(--font-line-height-normal) * 1em);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
    }

    /* The model's text, as written: line breaks kept, nothing else interpreted. */
    .text {
        margin: 0;
        min-height: calc(3 * var(--font-line-height-normal) * 1em);
        white-space: pre-wrap;
        overflow-wrap: anywhere;
        color: var(--color-text-primary);
    }

    .quiet {
        color: var(--color-text-tertiary);
    }

    .mark {
        margin-left: var(--spacing-xs);
        color: var(--color-text-tertiary);
    }

    .mark:first-child {
        margin-left: 0;
    }

    .unavailable {
        margin: 0;
        color: var(--color-text-secondary);
    }
</style>
