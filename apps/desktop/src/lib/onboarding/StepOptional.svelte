<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import OnboardingStepShell from './OnboardingStepShell.svelte'
    import SettingSwitch from '$lib/settings/components/SettingSwitch.svelte'
    import { setFooterOverride, requestWizardComplete } from './onboarding-state.svelte'

    /**
     * Step 3: Optional setup.
     *
     * Four toggles, each bound to an existing registry setting via `<SettingSwitch>`.
     * The switch component reads + writes the setting directly, so the toggles
     * live-apply the moment the user flips them: `network.enabled` /
     * `indexing.enabled` / `updates.autoCheck` / `fileOperations.mtpEnabled` all
     * have entries in `settings-applier.ts`'s `passthroughBackendHandlers` table that
     * fire the matching Rust-side helper.
     *
     * Defaults stay ON. Step 3's purpose is to let the user turn things OFF with full
     * context, not to ask for opt-in. See `lib/onboarding/CLAUDE.md` § "Step 3 (optional setup)".
     *
     * Footer: single primary "Start using Cmdr" button registered via
     * `setFooterOverride()`. Clicking it asks the wizard to finish: the wizard's
     * `onComplete` callback persists `isOnboarded: true` (via
     * `notifyOnboardingComplete`), drops the suppress-update-toast gate, and closes
     * the sheet. No safety-net persist call here: each switch already wrote its
     * setting on flip, so there's nothing pending to drain.
     */

    onMount(() => {
        // Footer button has no reactive deps (the click handler closes over module-level
        // functions only), so register once on mount rather than re-running an `$effect`.
        setFooterOverride([
            {
                label: 'Start using Cmdr',
                variant: 'primary',
                onclick: () => {
                    requestWizardComplete()
                },
            },
        ])
    })

    onDestroy(() => {
        // Clear the footer override so other steps' default buttons render again, and
        // so any teardown-then-remount (Vitest hot reload, future re-entry) doesn't
        // leak stale closures.
        setFooterOverride(null)
    })
</script>

<OnboardingStepShell>
    <h2 class="step-title">You're almost ready</h2>
    <p class="lede">
        You chose to walk through a detailed setup, so here are a few easy choices. If you don't care too much, just
        click the button below. These are all options, and the defaults are picked for your benefit.
    </p>

    <section class="toggle-block" aria-labelledby="toggle-networking-title">
        <header class="toggle-header">
            <div class="toggle-text">
                <h3 id="toggle-networking-title" class="toggle-title">Networking</h3>
                <p class="toggle-desc">
                    Having this <em>on</em> means you can connect to SMB servers like company network shares, a home
                    NAS, and the like. The only cost is a macOS permission dialog that pops up and asks you to allow
                    "Local network access", and one for "Accepting incoming connections". Both dialogs are harmless,
                    but if you don't know what these are, they might be scary or annoying.
                </p>
            </div>
            <div class="toggle-control">
                <SettingSwitch id="network.enabled" />
                <p class="toggle-caption">Recommended: on. You can change this any time in Settings.</p>
            </div>
        </header>
    </section>

    <section class="toggle-block" aria-labelledby="toggle-indexing-title">
        <header class="toggle-header">
            <div class="toggle-text">
                <h3 id="toggle-indexing-title" class="toggle-title">Drive indexing</h3>
                <p class="toggle-desc">
                    Drive indexing is totally cool! Gives you two main things:
                </p>
                <ol class="toggle-list">
                    <li>Instant search of your whole drive. Think Spotlight, but even faster.</li>
                    <li>
                        Real-time folder sizes for your whole drive. You always know how much stuff you have in each
                        folder.
                    </li>
                </ol>
                <p class="toggle-desc">
                    If you turn this off, you only get <code>&lt;DIR&gt;</code> for the sizes. The cost is a 300 MB
                    index on your drive, but no extra CPU or memory use after the first 2&ndash;3 minutes of you first
                    starting the app, or starting it after a long time. It's a cheap feature considering the benefits.
                </p>
            </div>
            <div class="toggle-control">
                <SettingSwitch id="indexing.enabled" />
                <p class="toggle-caption">Recommended: on. You can change this any time in Settings.</p>
            </div>
        </header>
    </section>

    <section class="toggle-block" aria-labelledby="toggle-updates-title">
        <header class="toggle-header">
            <div class="toggle-text">
                <h3 id="toggle-updates-title" class="toggle-title">Automatic updates</h3>
                <p class="toggle-desc">
                    If you enable this, Cmdr makes a tiny network request to a central license server at each app
                    start plus once every 24 hours, and you always get the latest updates. If disabled, you'll keep
                    your current version, and zero automated network requests (except for periodic license checks, if
                    you have a commercial license).
                </p>
            </div>
            <div class="toggle-control">
                <SettingSwitch id="updates.autoCheck" />
                <p class="toggle-caption">Recommended: on. You can change this any time in Settings.</p>
            </div>
        </header>
    </section>

    <section class="toggle-block" aria-labelledby="toggle-mtp-title">
        <header class="toggle-header">
            <div class="toggle-text">
                <h3 id="toggle-mtp-title" class="toggle-title">MTP (Android phones, Kindles, cameras)</h3>
                <p class="toggle-desc">
                    If you enable this, Cmdr can <strong>connect to Android phones, Kindles, cameras</strong>, some
                    music players, and any other device that supports the protocols called MTP or PTP. The cost is
                    that macOS <em>also</em> wants to connect to these (and it usually fails, which is why you can't
                    just use Finder to copy photos from Android phones), so Cmdr has to suppress that macOS process
                    while it's running. When you quit Cmdr, this is politely restored. But it's a bit of a cost, so:
                </p>
            </div>
            <div class="toggle-control">
                <SettingSwitch id="fileOperations.mtpEnabled" />
                <p class="toggle-caption">Recommended: on. You can change this any time in Settings.</p>
            </div>
        </header>
    </section>
</OnboardingStepShell>

<style>
    .step-title {
        margin: 0 0 var(--spacing-md);
        /* 20% larger than body font (same calc() as StepFda/.welcome and
           StepAi/.step-title so all onboarding step headings match). */
        font-size: calc(var(--font-size-md) * 1.2);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .lede {
        margin: 0 0 var(--spacing-lg);
        line-height: 1.5;
        color: var(--color-text-primary);
    }

    .toggle-block {
        margin-bottom: var(--spacing-md);
        padding: var(--spacing-lg);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        background: var(--color-bg-primary);
    }

    .toggle-block:last-child {
        margin-bottom: 0;
    }

    .toggle-header {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-lg);
    }

    .toggle-text {
        flex: 1;
        min-width: 0;
    }

    .toggle-control {
        flex-shrink: 0;
        display: flex;
        flex-direction: column;
        align-items: flex-end;
        gap: var(--spacing-xs);
        padding-top: var(--spacing-xxs);
    }

    .toggle-caption {
        margin: 0;
        max-width: 14rem;
        text-align: right;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        line-height: 1.4;
    }

    .toggle-list {
        margin: 0 0 var(--spacing-sm);
        padding-left: var(--spacing-lg);
        font-size: var(--font-size-sm);
        line-height: 1.5;
        color: var(--color-text-secondary);
    }

    .toggle-list li {
        margin: 0 0 var(--spacing-xxs);
    }

    .toggle-title {
        margin: 0 0 var(--spacing-xs);
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .toggle-desc {
        margin: 0;
        font-size: var(--font-size-sm);
        line-height: 1.5;
        color: var(--color-text-secondary);
    }

    .toggle-desc code {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        background: var(--color-bg-tertiary);
        padding: var(--spacing-xxs) var(--spacing-xs);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
    }
</style>
