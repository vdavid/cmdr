<!-- A small "how this is measured" note under a chart, so no number on the dashboard is a black box. -->
<script lang="ts">
    let { text }: { text: string } = $props()
</script>

<p class="mt-2 text-xs leading-relaxed text-text-tertiary">{text}</p>
