<script lang="ts">
    // Tier 2 fixture. Both occurrences below should render folded as «m0»: the one in this script block, and the one
    // in the template, which is the case that matters for M4.
    const marker = 'CMDR_M0_PROBE'
</script>

<p>{marker}</p>
<p>{tString('CMDR_M0_PROBE')}</p>
