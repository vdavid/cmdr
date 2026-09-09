<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import OnboardingStepShell from './OnboardingStepShell.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import InfoTip from '$lib/ui/InfoTip.svelte'
    import SettingRow from '$lib/settings/components/SettingRow.svelte'
    import SettingSwitch from '$lib/settings/components/SettingSwitch.svelte'
    import { setFooterOverride, requestWizardComplete } from './onboarding-state.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import type { Snippet } from 'svelte'

    /**
     * Step 3: Optional setup.
     *
     * Four toggles in one `<SectionCard>` of `<SettingRow>`s, exactly the grouping Settings
     * uses: a card frames a RUN of rows, and the rows divide themselves. Onboarding passes
     * its own friendlier `label` / `description` rather than the registry's, which is the
     * one place it diverges from a Settings page.
     *
     * Each row leads with a half-line summary and parks the full explanation behind an
     * `<InfoTip>` beside its title. Four rows of full prose turned the last step of
     * onboarding into a wall of text, which is the worst possible place for one: the user
     * is trying to get INTO the app. The summary carries the trade-off, and the tip is
     * there for whoever wants the why.
     *
     * The switch component reads + writes the setting directly, so the toggles live-apply
     * the moment the user flips them: `network.enabled` / `indexing.enabled` /
     * `updates.autoCheck` / `fileOperations.mtpEnabled` all have entries in
     * `settings-applier.ts`'s `passthroughBackendHandlers` table that fire the matching
     * Rust-side helper.
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

    const recommendedOn = $derived(tString('onboarding.stepOptional.recommendedOn'))

    /** Accessible name for a row's info glyph, which has no visible text of its own. */
    function moreAbout(topic: string): string {
        return tString('onboarding.moreAbout', { topic })
    }
</script>

{#snippet em(children: Snippet)}<em>{@render children()}</em>{/snippet}
{#snippet strong(children: Snippet)}<strong>{@render children()}</strong>{/snippet}
{#snippet code(children: Snippet)}<code>{@render children()}</code>{/snippet}

<OnboardingStepShell>
    <h2 class="step-title">{tString('onboarding.stepOptional.title')}</h2>
    <p class="lede">{tString('onboarding.stepOptional.lede')}</p>

    <SectionCard>
        <SettingRow
            id="network.enabled"
            label={tString('onboarding.stepOptional.networking.title')}
            description={tString('onboarding.stepOptional.networking.summary')}
        >
            {#snippet labelTrailing()}
                <InfoTip label={moreAbout(tString('onboarding.stepOptional.networking.title'))}>
                    <p class="toggle-desc"><Trans key="onboarding.stepOptional.networking.desc" snippets={{ em }} /></p>
                    <p class="toggle-desc">{tString('onboarding.stepOptional.changeAnytime')}</p>
                </InfoTip>
            {/snippet}
            <div class="row-control">
                <!-- The verdict reads as the switch's own label rather than a footnote under it. -->
                <span class="row-caption">{recommendedOn}</span>
                <SettingSwitch id="network.enabled" />
            </div>
        </SettingRow>

        <SettingRow
            id="indexing.enabled"
            label={tString('onboarding.stepOptional.indexing.title')}
            description={tString('onboarding.stepOptional.indexing.summary')}
        >
            {#snippet labelTrailing()}
                <InfoTip label={moreAbout(tString('onboarding.stepOptional.indexing.title'))}>
                    <p class="toggle-desc">{tString('onboarding.stepOptional.indexing.descIntro')}</p>
                    <ol class="toggle-list">
                        <li>{tString('onboarding.stepOptional.indexing.benefit1')}</li>
                        <li>{tString('onboarding.stepOptional.indexing.benefit2')}</li>
                    </ol>
                    <!-- The folder-size placeholder comes from the file list's own catalog entry, so
                         this sentence can never name a placeholder the Size column doesn't show. -->
                    <p class="toggle-desc">
                        <Trans
                            key="onboarding.stepOptional.indexing.descCost"
                            snippets={{ code }}
                            params={{ dirPlaceholder: tString('fileExplorer.dirSize.dirPlaceholder') }}
                        />
                    </p>
                    <p class="toggle-desc">{tString('onboarding.stepOptional.changeAnytime')}</p>
                </InfoTip>
            {/snippet}
            <div class="row-control">
                <!-- The verdict reads as the switch's own label rather than a footnote under it. -->
                <span class="row-caption">{recommendedOn}</span>
                <SettingSwitch id="indexing.enabled" />
            </div>
        </SettingRow>

        <SettingRow
            id="updates.autoCheck"
            label={tString('onboarding.stepOptional.updates.title')}
            description={tString('onboarding.stepOptional.updates.summary')}
        >
            {#snippet labelTrailing()}
                <InfoTip label={moreAbout(tString('onboarding.stepOptional.updates.title'))}>
                    <p class="toggle-desc">{tString('onboarding.stepOptional.updates.desc')}</p>
                    <p class="toggle-desc">{tString('onboarding.stepOptional.changeAnytime')}</p>
                </InfoTip>
            {/snippet}
            <div class="row-control">
                <!-- The verdict reads as the switch's own label rather than a footnote under it. -->
                <span class="row-caption">{recommendedOn}</span>
                <SettingSwitch id="updates.autoCheck" />
            </div>
        </SettingRow>

        <SettingRow
            id="fileOperations.mtpEnabled"
            label={tString('onboarding.stepOptional.mtp.title')}
            description={tString('onboarding.stepOptional.mtp.summary')}
        >
            {#snippet labelTrailing()}
                <InfoTip label={moreAbout(tString('onboarding.stepOptional.mtp.title'))}>
                    <p class="toggle-desc"><Trans key="onboarding.stepOptional.mtp.desc" snippets={{ strong, em }} /></p>
                    <p class="toggle-desc">{tString('onboarding.stepOptional.changeAnytime')}</p>
                </InfoTip>
            {/snippet}
            <div class="row-control">
                <!-- The verdict reads as the switch's own label rather than a footnote under it. -->
                <span class="row-caption">{recommendedOn}</span>
                <SettingSwitch id="fileOperations.mtpEnabled" />
            </div>
        </SettingRow>
    </SectionCard>
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
        line-height: var(--font-line-height-prose);
        color: var(--color-text-primary);
    }

    .row-control {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .row-caption {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        white-space: nowrap;
    }

    /* Inside the rows' info tips, so these stay parent-scoped. The list sits in the same
       vertical rhythm as the paragraphs around it: one paragraph gap above and below,
       never a bigger one on one side. */
    .toggle-list {
        margin: 0 0 var(--spacing-md);
        padding-left: 0;
        list-style: none;
        counter-reset: toggle-benefit;
        font-size: var(--font-size-sm);
        line-height: var(--font-line-height-prose);
        color: var(--color-text-secondary);
    }

    /* Numbers line up with the paragraph text above rather than sitting in an indent of
       their own; the hanging indent keeps a wrapped line under the words, not the number.
       ❌ Not `text-indent`, which inherits into inline-flex descendants and displaces
       their content (it broke `ShortcutChip` on step 3). See `StepBeta`'s feedback list. */
    .toggle-list li {
        display: flex;
        gap: var(--spacing-xs);
        margin: 0 0 var(--spacing-xxs);
        counter-increment: toggle-benefit;
    }

    .toggle-list li::before {
        content: counter(toggle-benefit) '.';
        flex: none;
    }

    /* `pre-line` is what turns the sentence-per-line newlines in the catalog into actual
       line breaks: a tooltip has the vertical room, and one unbroken block of six
       sentences is the wall of text the info glyph existed to avoid. */
    .toggle-desc {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-sm);
        line-height: var(--font-line-height-prose);
        color: var(--color-text-secondary);
        white-space: pre-line;
    }

    .toggle-desc:last-child {
        margin-bottom: 0;
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
