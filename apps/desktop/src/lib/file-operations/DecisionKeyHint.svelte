<!--
  The letter that answers a decision prompt, shown inside its button. Hidden from
  assistive tech: the button carries `aria-keyshortcuts`, and the letter read as
  part of its name ("Skip S") would only garble it. Letters: `decision-keys.ts`.
-->
<script lang="ts">
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'

    interface Props {
        /** The fixed letter, from `CONFLICT_KEYS` / `ERROR_KEYS`. */
        key: string
    }

    const { key }: Props = $props()
</script>

<span class="decision-key-hint" aria-hidden="true"><ShortcutChip {key} size="sm" /></span>

<style>
    .decision-key-hint {
        display: inline-flex;
        margin-inline-start: var(--spacing-sm);
        vertical-align: middle;
    }
</style>
