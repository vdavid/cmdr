<!--
  Renders a catalog message that contains inline components, at the locale's word
  order, with each `<tag>` replaced by a real Svelte snippet (NOT `{@html}`, so
  it's XSS-safe by construction: text stays text, components are components).

  Usage:

    <Trans key="common.downloadsFdaHint" snippets={{ settingsLink }} />

  with `settingsLink` a `Snippet<[Snippet]>` that wraps its inner content, e.g.

    {#snippet settingsLink(children)}
      <LinkButton onclick={open}>{@render children()}</LinkButton>
    {/snippet}

  Mechanism: each `<tag>` in the message is supplied to `intl-messageformat`'s
  `format()` as a handler function returning a marker object; the engine returns
  an array of plain strings and markers, which we render as text nodes + the
  matching snippet. The snippet receives the tag's inner chunks as a child
  snippet so styled/interactive wrappers keep their content.
-->
<script lang="ts">
    import type { Snippet } from 'svelte'
    import { t, type TranslationParams } from './messages.svelte'
    import type { MessageKey } from './keys.gen'

    interface Marker {
        __trans: true
        tag: string
        chunks: unknown[]
    }

    interface Props {
        key: MessageKey
        /** One snippet per `<tag>` in the message. Each takes the tag's inner content as a child snippet. */
        snippets?: Record<string, Snippet<[Snippet]>>
        /** Plain interpolation params (`{name}`), alongside the tag snippets. */
        params?: TranslationParams
    }

    const { key, snippets = {}, params }: Props = $props()

    function isMarker(part: unknown): part is Marker {
        return typeof part === 'object' && part !== null && '__trans' in part
    }

    // Build tag handlers: each `<tag>` returns a marker the renderer matches to a snippet.
    const parts = $derived.by((): unknown[] => {
        const handlers: TranslationParams = { ...params }
        for (const tag of Object.keys(snippets)) {
            handlers[tag] = (chunks: unknown[]): Marker => ({ __trans: true, tag, chunks })
        }
        const result = t(key, handlers)
        // A message with tags formats to an array of strings + markers; one with
        // no tags formats to a plain string (or, defensively, any non-array).
        // Normalize to an array either way.
        return Array.isArray(result) ? (result as unknown[]) : [result]
    })
</script>

<!--
  Render each message part: plain strings as text nodes, tag markers via the
  matching consumer snippet. The consumer receives an inner `content` snippet
  (defined per-part below, closing over `part.chunks`) and renders it with
  `{@render content()}`, so the tag's inner text lands inside the component.
-->
<!-- eslint-disable-next-line svelte/require-each-key -- positional message parts have no stable id; order is the identity -->
{#each parts as part}{#if isMarker(part)}{@const childSnippet = snippets[part.tag]}{#if childSnippet}{#snippet content()}{#each part.chunks as chunk}{chunk}{/each}{/snippet}{@render childSnippet(content)}{/if}{:else}{part}{/if}{/each}
