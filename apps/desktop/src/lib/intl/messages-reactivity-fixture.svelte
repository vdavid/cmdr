<!--
  Test-only fixture for the reactivity guard in `messages.svelte.test.ts`.
  Reads `t()` in MARKUP so a `setLocale()` rune bump must re-render the text.
  Kept as a real component (not `createRawSnippet`) so the test exercises the
  exact reactive path real call sites use.
-->
<script lang="ts">
    import { tString } from './messages.svelte'
    import type { MessageKey } from './keys.gen'

    interface Props {
        messageKey: MessageKey
    }

    const { messageKey }: Props = $props()
</script>

<span data-test="trans-text">{tString(messageKey, { countText: '1', count: 1 })}</span>
