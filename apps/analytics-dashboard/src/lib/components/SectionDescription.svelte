<!--
  A section's "what is this, and how much to trust it" blurb, shown right under the section heading.
  `insight` is the one-line "use this to..." purpose; `caveat` is the reliability footnote (optional).
-->
<script lang="ts">
    const { insight, caveat }: { insight: string; caveat?: string } = $props()
</script>

<p class="mb-4 text-xs/relaxed text-text-secondary">
    {insight}
    {#if caveat}<span class="text-text-tertiary">{caveat}</span>{/if}
</p>
