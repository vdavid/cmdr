<script lang="ts">
    import OnboardingStepShell from './OnboardingStepShell.svelte'

    /**
     * M1 placeholder. M4 wires the four optional toggles (networking, drive indexing,
     * automatic updates, MTP) with their long-form descriptions, all bound to existing
     * registry settings. See `docs/specs/onboarding-revamp-plan.md` § M4.
     */
</script>

<OnboardingStepShell>
    <h2 class="step-title">Step 3 — Optional setup</h2>
    <p class="step-placeholder">M4 wires the four optional toggles here.</p>
</OnboardingStepShell>

<style>
    .step-title {
        margin: 0 0 var(--spacing-md) 0;
        font-size: var(--font-size-lg);
        font-weight: 600;
    }

    .step-placeholder {
        margin: 0;
        color: var(--color-text-secondary);
    }
</style>
