<!--
  Test-only host for the `<Trans>` proof: renders the FDA-hint
  message with its `<settingsLink>` tag mapped to a real interactive
  `LinkButton`, exactly as a real call site would. The parent test asserts the
  text + component render in order and that the click handler fires.
-->
<script lang="ts">
    import type { Snippet } from 'svelte'
    import Trans from './Trans.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import type { MessageKey } from './keys.gen'

    interface Props {
        messageKey: MessageKey
        onLinkClick?: () => void
    }

    const { messageKey, onLinkClick }: Props = $props()
</script>

{#snippet settingsLink(children: Snippet)}
    <LinkButton onclick={onLinkClick}>{@render children()}</LinkButton>
{/snippet}

<p data-test="trans-host"><Trans key={messageKey} snippets={{ settingsLink }} /></p>
