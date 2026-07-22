<script lang="ts">
    import { onMount } from 'svelte'
    import Checkbox from '$lib/ui/Checkbox.svelte'

    let isDarkMode = $state(true)

    onMount(async () => {
        if (typeof window !== 'undefined') {
            isDarkMode = window.matchMedia('(prefers-color-scheme: dark)').matches
        }
        try {
            const { getCurrentWindow } = await import('@tauri-apps/api/window')
            const theme = await getCurrentWindow().theme()
            if (theme) {
                isDarkMode = theme === 'dark'
            }
        } catch {
            // Not in Tauri or theme not set
        }
    })

    async function handleThemeToggle() {
        isDarkMode = !isDarkMode
        try {
            const { setTheme } = await import('@tauri-apps/api/app')
            await setTheme(isDarkMode ? 'dark' : 'light')
        } catch (error) {
            // eslint-disable-next-line no-console -- Debug window is dev-only
            console.error('Failed to set theme:', error)
        }
    }
</script>

<section class="debug-section">
    <h2>Appearance</h2>
    <div class="toggle-row">
        <span>Dark mode</span>
        <Checkbox checked={isDarkMode} onCheckedChange={handleThemeToggle} ariaLabel="Dark mode" />
    </div>
</section>
