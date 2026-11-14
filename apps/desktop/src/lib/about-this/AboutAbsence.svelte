<!--
  One row's "no answer": a Routine unknown or a question with no answer here as one short muted line, a Notable
  unknown as a full sentence with a quiet mark. The prominence arrives typed from Rust; this only words it.
-->
<script lang="ts">
    import type { FactKind, NotApplicableWhy, Unknown } from '#lib/ipc/bindings.js'
    import { t } from '#lib/intl/messages.svelte.js'
    import Icon from '#lib/ui/Icon.svelte'
    import { NOT_APPLICABLE_WHY, absenceKey } from './about-words'

    interface Props {
        kind: FactKind
        unknown?: Unknown
        notApplicable?: NotApplicableWhy
    }

    const { kind, unknown, notApplicable }: Props = $props()

    const key = $derived(
        unknown ? absenceKey(kind, unknown.why) : NOT_APPLICABLE_WHY[notApplicable ?? 'volumeHasNone'],
    )
    const notable = $derived(unknown?.prominence === 'notable')
</script>

{#if notable}
    <span class="notable"><span class="mark" aria-hidden="true"><Icon name="circle-alert" size={12} /></span>{t(key)}</span>
{:else}
    <span class="quiet">{t(key)}</span>
{/if}

<style>
    .quiet {
        color: var(--color-text-tertiary);
    }

    .notable {
        display: inline-flex;
        gap: var(--spacing-xs);
        align-items: baseline;
    }

    .mark {
        flex: none;
        color: var(--color-warning-text);
        position: relative;
        top: 1px; /* An optical nudge onto the text baseline. */
    }
</style>
