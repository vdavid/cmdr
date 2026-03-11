<script lang="ts">
    import { onMount } from 'svelte'
    import { getCachedStatus } from './licensing-store.svelte'
    import { openExternalUrl } from '$lib/tauri-commands'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'

    interface Props {
        onClose: () => void
    }

    const { onClose }: Props = $props()

    // Get license info
    const status = getCachedStatus()

    // Version will be loaded from Tauri
    let version = $state('0.0.0')

    onMount(async () => {
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

    // Determine if we should show the license purchase link
    function shouldShowLicenseLink(): boolean {
        if (!status) return true
        if (status.type === 'personal') return true
        if (status.type === 'expired') return true
        return false
    }

    function handleLinkClick(url: string) {
        return (event: MouseEvent) => {
            event.preventDefault()
            void openExternalUrl(url)
        }
    }
</script>

<ModalDialog
    titleId="about-title"
    blur
    dialogId="about"
    onclose={onClose}
    containerStyle="min-width: 380px; max-width: 480px"
>
    {#snippet title()}
        <!-- Title is visually hidden, app name serves as the visual title -->
        <span class="sr-only">About Cmdr</span>
    {/snippet}

    <div class="about-body">
        <div class="about-content">
            <div class="app-icon">
                <span class="icon-text">⌘</span>
            </div>

            <p class="app-name">Cmdr</p>
            <p class="app-tagline">Keyboard-driven file manager</p>

            <div class="version-info">
                <span class="version">Version {version}</span>
            </div>

            <div class="license-info">
                <p class="license-description">{getLicenseDescription()}</p>
            </div>

            <p class="ai-attribution">AI powered by Falcon-H1R-7B by Technology Innovation Institute (TII)</p>

            <div class="links">
                <a href="https://getcmdr.com" onclick={handleLinkClick('https://getcmdr.com')}>Website</a>
                {#if shouldShowLicenseLink()}
                    <span class="separator">•</span>
                    <a href="https://getcmdr.com/pricing" onclick={handleLinkClick('https://getcmdr.com/pricing')}
                        >Get a license</a
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
</ModalDialog>

<style>
    .sr-only {
        position: absolute;
        width: 1px;
        height: 1px;
        padding: 0;
        margin: -1px;
        overflow: hidden;
        clip-path: inset(50%);
        white-space: nowrap;
        border: 0;
    }

    .about-body {
        padding: 0 var(--spacing-2xl) var(--spacing-2xl);
    }

    .about-content {
        text-align: center;
    }

    .app-icon {
        width: 80px;
        height: 80px;
        margin: 0 auto var(--spacing-lg);
        background: linear-gradient(135deg, #4a9eff, #7c3aed);
        border-radius: var(--radius-lg);
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
        margin: 0 0 var(--spacing-xs);
        color: var(--color-text-primary);
    }

    .app-tagline {
        color: var(--color-text-secondary);
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
    }

    .version-info {
        margin-bottom: 20px;
    }

    .version {
        color: var(--color-text-secondary);
        font-size: var(--font-size-md);
    }

    .license-info {
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-lg);
        padding: var(--spacing-lg) 20px;
        margin-bottom: 20px;
    }

    .license-description {
        color: var(--color-text-primary);
        font-size: var(--font-size-md);
        line-height: 1.5;
        margin: 0;
    }

    .ai-attribution {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        margin: 0 0 var(--spacing-lg);
    }

    .links {
        margin-bottom: 16px;
    }

    .links a {
        color: var(--color-accent);
        text-decoration: none;
        font-size: var(--font-size-md);
    }

    .links a:hover {
        text-decoration: underline;
    }

    .separator {
        color: var(--color-text-secondary);
        margin: 0 8px;
    }

    .copyright {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        margin: 0;
    }
</style>
