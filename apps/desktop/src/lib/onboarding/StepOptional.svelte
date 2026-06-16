<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import OnboardingStepShell from './OnboardingStepShell.svelte'
    import SettingSwitch from '$lib/settings/components/SettingSwitch.svelte'
    import { setFooterOverride, requestWizardComplete } from './onboarding-state.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import type { Snippet } from 'svelte'

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
                label: tString('onboarding.stepOptional.footer.start'),
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

{#snippet em(children: Snippet)}<em>{@render children()}</em>{/snippet}
{#snippet strong(children: Snippet)}<strong>{@render children()}</strong>{/snippet}
{#snippet code(children: Snippet)}<code>{@render children()}</code>{/snippet}

<OnboardingStepShell>
    <h2 class="step-title">{tString('onboarding.stepOptional.title')}</h2>
    <p class="lede">{tString('onboarding.stepOptional.lede')}</p>

    <section class="toggle-block" aria-labelledby="toggle-networking-title">
        <header class="toggle-header">
            <div class="toggle-text">
                <h3 id="toggle-networking-title" class="toggle-title">{tString('onboarding.stepOptional.networking.title')}</h3>
                <p class="toggle-desc"><Trans key="onboarding.stepOptional.networking.desc" snippets={{ em }} /></p>
            </div>
            <div class="toggle-control">
                <SettingSwitch id="network.enabled" />
                <p class="toggle-caption">{tString('onboarding.stepOptional.recommendedOn')}</p>
            </div>
        </header>
    </section>

    <section class="toggle-block" aria-labelledby="toggle-indexing-title">
        <header class="toggle-header">
            <div class="toggle-text">
                <h3 id="toggle-indexing-title" class="toggle-title">{tString('onboarding.stepOptional.indexing.title')}</h3>
                <p class="toggle-desc">{tString('onboarding.stepOptional.indexing.descIntro')}</p>
                <ol class="toggle-list">
                    <li>{tString('onboarding.stepOptional.indexing.benefit1')}</li>
                    <li>{tString('onboarding.stepOptional.indexing.benefit2')}</li>
                </ol>
                <p class="toggle-desc"><Trans key="onboarding.stepOptional.indexing.descCost" snippets={{ code }} /></p>
            </div>
            <div class="toggle-control">
                <SettingSwitch id="indexing.enabled" />
                <p class="toggle-caption">{tString('onboarding.stepOptional.recommendedOn')}</p>
            </div>
        </header>
    </section>

    <section class="toggle-block" aria-labelledby="toggle-updates-title">
        <header class="toggle-header">
            <div class="toggle-text">
                <h3 id="toggle-updates-title" class="toggle-title">{tString('onboarding.stepOptional.updates.title')}</h3>
                <p class="toggle-desc">{tString('onboarding.stepOptional.updates.desc')}</p>
            </div>
            <div class="toggle-control">
                <SettingSwitch id="updates.autoCheck" />
                <p class="toggle-caption">{tString('onboarding.stepOptional.recommendedOn')}</p>
            </div>
        </header>
    </section>

    <section class="toggle-block" aria-labelledby="toggle-mtp-title">
        <header class="toggle-header">
            <div class="toggle-text">
                <h3 id="toggle-mtp-title" class="toggle-title">{tString('onboarding.stepOptional.mtp.title')}</h3>
                <p class="toggle-desc"><Trans key="onboarding.stepOptional.mtp.desc" snippets={{ strong, em }} /></p>
            </div>
            <div class="toggle-control">
                <SettingSwitch id="fileOperations.mtpEnabled" />
                <p class="toggle-caption">{tString('onboarding.stepOptional.recommendedOn')}</p>
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
