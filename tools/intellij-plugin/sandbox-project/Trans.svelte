<!-- A stand-in for the real `<Trans>`, so `Sample.svelte` resolves its import and reads without red. -->
<script lang="ts">
  let { key }: { key: string } = $props()
</script>

<span>{key}</span>
