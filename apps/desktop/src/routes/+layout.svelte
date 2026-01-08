<script lang="ts">
    import { onMount } from 'svelte'
    import '../app.css'
    import { initWindowStateListener } from '$lib/window-state'

    onMount(() => {
        // Initialize window state persistence on resize
        // This ensures window size/position survives hot reloads
        void initWindowStateListener()
    })
</script>

<slot />
