<script lang="ts">
    import { onMount, tick } from 'svelte'
    import { getCachedStatus } from '$lib/licensing-store.svelte'
    import { openExternalUrl } from '$lib/tauri-commands'

    /** Props */
    interface Props {
        onClose: () => void
    }

    const { onClose }: Props = $props()

    // Get license info
    const status = getCachedStatus()

    // Version will be loaded from Tauri
    let version = $state('0.0.0')
    let overlayElement: HTMLDivElement | undefined = $state()

    onMount(async () => {
        // Focus overlay so keyboard events work immediately
        void tick().then(() => {
            overlayElement?.focus()
        })

        try {
            const { getVersion } = await import('@tauri-apps/api/app')
            version = await getVersion()
        } catch {
            // Not in Tauri environment
        }
    })

    // Format expiration date nicely
    function formatDate(dateStr: string | null | undefined): string {
        if (!dateStr) return ''
        try {
            return new Date(dateStr).toLocaleDateString(undefined, {
                year: 'numeric',
                month: 'long',
                day: 'numeric',
            })
        } catch {
            return dateStr
        }
    }

    // Get descriptive license text
    function getLicenseDescription(): string {
        if (!status) {
            return 'No license – only personal use allowed'
        }
        switch (status.type) {
            case 'supporter':
                return 'Personal license – thanks for your support ❤️'
            case 'commercial':
                if (status.licenseType === 'commercial_perpetual') {
                    return `Perpetual commercial license for ${status.organizationName || 'your organization'}`
                } else {
                    const expiresAt = formatDate(status.expiresAt)
                    return `Commercial license for ${status.organizationName || 'your organization'}${expiresAt ? `, valid until ${expiresAt}` : ''}`
                }
            case 'expired': // Fallthrough, no shaming needed for the expired license
            case 'personal':
            default:
                return 'No license – only personal use allowed'
        }
    }

    // Determine if we should show the Upgrade link
    function shouldShowUpgradeLink(): boolean {
        if (!status) return true // No license - show upgrade
        if (status.type === 'personal') return true
        if (status.type === 'expired') return true
        // Supporter and commercial don't show the generic upgrade link
        return false
    }

    // Determine if we should show the commercial upgrade prompt (for supporters)
    function shouldShowCommercialPrompt(): boolean {
        return status?.type === 'supporter'
    }

    function handleKeydown(event: KeyboardEvent) {
        // Stop propagation to prevent file explorer from handling keys while modal is open
        event.stopPropagation()
        if (event.key === 'Escape') {
            onClose()
        }
    }

    function handleLinkClick(url: string) {
        return (event: MouseEvent) => {
            event.preventDefault()
            void openExternalUrl(url)
        }
    }
</script>

<div
    bind:this={overlayElement}
    class="about-overlay"
    role="dialog"
    aria-modal="true"
    aria-labelledby="about-title"
    tabindex="-1"
    onkeydown={handleKeydown}
>
    <div class="about-window">
        <button class="close-button" onclick={onClose} aria-label="Close">×</button>

        <div class="about-content">
            <div class="app-icon">
                <!-- App icon placeholder -->
                <span class="icon-text">⌘</span>
            </div>

            <h1 id="about-title" class="app-name">Cmdr</h1>
            <p class="app-tagline">Keyboard-driven file manager</p>

            <div class="version-info">
                <span class="version">Version {version}</span>
            </div>

            <div class="license-info">
                <p class="license-description">{getLicenseDescription()}</p>
                {#if shouldShowCommercialPrompt()}
                    <p class="commercial-prompt">
                        Also using Cmdr for work? You must <a
                            href="https://getcmdr.com/pricing"
                            onclick={handleLinkClick('https://getcmdr.com/pricing')}>upgrade to a commercial license</a
                        >.
                    </p>
                {/if}
            </div>

            <div class="links">
                <a href="https://getcmdr.com" onclick={handleLinkClick('https://getcmdr.com')}>Website</a>
                {#if shouldShowUpgradeLink()}
                    <span class="separator">•</span>
                    <a href="https://getcmdr.com/pricing" onclick={handleLinkClick('https://getcmdr.com/pricing')}
                        >Upgrade</a
                    >
                {/if}
                <span class="separator">•</span>
                <a href="https://github.com/vdavid/cmdr" onclick={handleLinkClick('https://github.com/vdavid/cmdr')}
                    >GitHub</a
                >
            </div>

            <p class="copyright">© 2024-2026 David Veszelovszki</p>
        </div>
    </div>
</div>

<style>
    .about-overlay {
        position: fixed;
        inset: 0;
        background: rgba(0, 0, 0, 0.6);
        backdrop-filter: blur(4px);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 9999;
    }

    .about-window {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-primary);
        border-radius: 12px;
        padding: 32px;
        min-width: 380px;
        max-width: 480px;
        position: relative;
        box-shadow: 0 20px 60px rgba(0, 0, 0, 0.4);
    }

    .close-button {
        position: absolute;
        top: 12px;
        right: 12px;
        background: none;
        border: none;
        color: var(--color-text-secondary);
        font-size: 24px;
        cursor: pointer;
        padding: 4px 8px;
        line-height: 1;
        border-radius: 4px;
    }

    .close-button:hover {
        background: var(--color-button-hover);
        color: var(--color-text-primary);
    }

    .about-content {
        text-align: center;
    }

    .app-icon {
        width: 80px;
        height: 80px;
        margin: 0 auto 16px;
        background: linear-gradient(135deg, #4a9eff, #7c3aed);
        border-radius: 16px;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .icon-text {
        font-size: 40px;
        color: white;
    }

    .app-name {
        font-size: 28px;
        font-weight: 600;
        margin: 0 0 4px;
        color: var(--color-text-primary);
    }

    .app-tagline {
        color: var(--color-text-secondary);
        margin: 0 0 16px;
        font-size: 14px;
    }

    .version-info {
        margin-bottom: 20px;
    }

    .version {
        color: var(--color-text-secondary);
        font-size: 13px;
    }

    .license-info {
        background: var(--color-bg-tertiary);
        border-radius: 8px;
        padding: 16px 20px;
        margin-bottom: 20px;
    }

    .license-description {
        color: var(--color-text-primary);
        font-size: 14px;
        line-height: 1.5;
        margin: 0;
    }

    .commercial-prompt {
        color: var(--color-text-secondary);
        font-size: 13px;
        line-height: 1.5;
        margin: 12px 0 0;
    }

    .commercial-prompt a {
        color: var(--color-accent);
        text-decoration: underline;
    }

    .commercial-prompt a:hover {
        color: var(--color-accent-hover);
    }

    .links {
        margin-bottom: 16px;
    }

    .links a {
        color: var(--color-accent);
        text-decoration: none;
        font-size: 13px;
    }

    .links a:hover {
        text-decoration: underline;
    }

    .separator {
        color: var(--color-text-secondary);
        margin: 0 8px;
    }

    .copyright {
        color: var(--color-text-muted);
        font-size: 12px;
        margin: 0;
    }
</style>
