<script lang="ts">
    import { onMount } from 'svelte'
    import { getCachedStatus } from './licensing-store.svelte'
    import { openExternalUrl } from '$lib/tauri-commands'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { GITHUB_ISSUES_URL } from '$lib/beta-links'

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
            return tString('licensing.about.noLicense')
        }
        switch (status.type) {
            case 'commercial': {
                const org = status.organizationName || tString('licensing.about.fallbackOrg')
                if (status.licenseType === 'commercial_perpetual') {
                    return tString('licensing.about.perpetual', { org })
                }
                const expiresAt = formatDate(status.expiresAt)
                return expiresAt
                    ? tString('licensing.about.commercialUntil', { org, date: expiresAt })
                    : tString('licensing.about.commercial', { org })
            }
            case 'expired': // Fallthrough, no shaming needed for the expired license
            case 'personal':
            default:
                return tString('licensing.about.noLicense')
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

{#snippet github(children: import('svelte').Snippet)}
    <LinkButton
        href={GITHUB_ISSUES_URL}
        target="_blank"
        rel="noopener noreferrer"
        onclick={handleLinkClick(GITHUB_ISSUES_URL)}>{@render children()}</LinkButton
    >
{/snippet}

<ModalDialog
    titleId="about-title"
    blur
    dialogId="about"
    onclose={onClose}
    containerStyle="min-width: 380px; max-width: 480px"
>
    {#snippet title()}
        <!-- Title is visually hidden, app name serves as the visual title -->
        <span class="sr-only">{tString('licensing.about.srTitle')}</span>
    {/snippet}

    <div class="about-body">
        <div class="about-content">
            <div class="app-icon">
                <span class="icon-text">⌘</span>
            </div>

            <p class="app-name">{tString('licensing.about.appName')}</p>
            <p class="app-tagline">{tString('licensing.about.tagline')}</p>

            <div class="version-info">
                <span class="version">{tString('licensing.about.version', { version })}</span>
                <p class="beta-note">
                    <Trans key="licensing.about.betaNote" snippets={{ github }} />
                </p>
            </div>

            <div class="license-info">
                <p class="license-description">{getLicenseDescription()}</p>
            </div>

            <p class="ai-attribution">{tString('licensing.about.aiAttribution')}</p>

            <div class="links">
                <a href="https://getcmdr.com" onclick={handleLinkClick('https://getcmdr.com')}
                    >{tString('licensing.about.linkWebsite')}</a
                >
                {#if shouldShowLicenseLink()}
                    <span class="separator">•</span>
                    <a href="https://getcmdr.com/pricing" onclick={handleLinkClick('https://getcmdr.com/pricing')}
                        >{tString('licensing.about.linkGetLicense')}</a
                    >
                {/if}
                <span class="separator">•</span>
                <a href="https://github.com/vdavid/cmdr" onclick={handleLinkClick('https://github.com/vdavid/cmdr')}
                    >{tString('licensing.about.linkGithub')}</a
                >
                <span class="separator">•</span>
                <a href="https://discord.gg/4BVafBneKJ" onclick={handleLinkClick('https://discord.gg/4BVafBneKJ')}
                    >{tString('licensing.about.linkDiscord')}</a
                >
            </div>

            <p class="copyright">{tString('licensing.about.copyright')}</p>
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
        background: linear-gradient(135deg, var(--color-cmdr-blue), var(--color-cmdr-purple));
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
        margin-bottom: var(--spacing-xl);
    }

    .version {
        color: var(--color-text-secondary);
        font-size: var(--font-size-md);
    }

    .beta-note {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .license-info {
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-lg);
        padding: var(--spacing-lg) var(--spacing-xl);
        margin-bottom: var(--spacing-xl);
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
        margin-bottom: var(--spacing-lg);
    }

    .links a {
        color: var(--color-accent-text);
        text-decoration: none;
        font-size: var(--font-size-md);
    }

    .links a:hover {
        text-decoration: underline;
    }

    .separator {
        color: var(--color-text-secondary);
        margin: 0 var(--spacing-sm);
    }

    .copyright {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        margin: 0;
    }
</style>
