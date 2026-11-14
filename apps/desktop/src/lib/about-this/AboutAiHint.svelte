<!--
  The card's one line about AI when a setting stands between it and an AI take: AI off, cloud AI not allowed yet, or no
  key. It says what turning AI on adds and links to the setting, under the footer's buttons (`ModalDialog`'s
  `footerNote`), so the facts keep the whole body.

  ❗ Its × hides it for good (`aboutThis.aiHintDismissed`, whose Settings > Advanced row is the only way back). The card's
  host persists that; this component only hides itself at the click, so the line goes at once.

  ❌ No one-click allow: the consent link opens Settings at the disclosure, where the whole thing is read.
-->
<script lang="ts">
    import { tString } from '#lib/intl/messages.svelte.js'
    import CloseButton from '#lib/ui/CloseButton.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import LinkButton from '#lib/ui/LinkButton.svelte'
    import { openCloudConsentSettings } from '#lib/ai/cloud-consent.svelte.js'
    import { openSettingsWindow } from '#lib/settings/settings-window.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import type { HintWhy } from './about-verdict'
    import { HINT_LINE } from './about-words'

    interface Props {
        why: HintWhy
        /** The ×: remember it's gone. */
        onDismiss: () => void
    }

    const { why, onDismiss }: Props = $props()

    const log = getAppLogger('aboutThis')

    /** Flipped by the ×, so the line goes at the click, whatever the host does next. */
    let dismissed = $state(false)

    function openSetting(): void {
        if (why.kind === 'noCloudConsent') {
            openCloudConsentSettings('about-this-card')
            return
        }
        openSettingsWindow('about-this-card', ['AI', 'Provider']).catch((e: unknown) => {
            log.warn('opening the AI settings failed: {error}', { error: e })
        })
    }

    function dismiss(): void {
        dismissed = true
        onDismiss()
    }
</script>

{#if !dismissed}
    <p class="ai-hint" data-ai-hint={why.kind}>
        <span class="hint-glyph" aria-hidden="true"><Icon name="info" size={13} /></span>
        <span class="hint-text">
            {tString(HINT_LINE[why.kind])}
            <LinkButton onclick={openSetting}>{tString('aboutThis.verdict.unavailable.openAiSettings')}</LinkButton>
        </span>
        <CloseButton
            class="hint-dismiss"
            ariaLabel={tString('aboutThis.verdict.hint.dismiss')}
            tooltipText={tString('aboutThis.verdict.hint.dismissTooltip')}
            onclick={dismiss}
        />
    </p>
{/if}

<style>
    .ai-hint {
        display: flex;
        gap: var(--spacing-xs);
        align-items: center;
        margin: 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
    }

    .hint-glyph {
        flex: none;
        display: inline-flex;
        color: var(--color-text-tertiary);
    }

    .hint-text {
        flex: 1 1 auto;
        min-width: 0;
    }
</style>
