<script lang="ts">
    import OnboardingStepShell from './OnboardingStepShell.svelte'

    /**
     * M1 placeholder. M3 ships the AI provider radio, the cloud-provider picker
     * (`CloudProviderPicker.svelte`), the per-provider tutorial (`CloudProviderSetup.svelte`),
     * the three FDA-outcome banner branches, and the local-model background download.
     * See `docs/specs/onboarding-revamp-plan.md` § M3.
     */
</script>

<OnboardingStepShell>
    <h2 class="step-title">Step 2 — AI</h2>
    <p class="step-placeholder">M3 wires the provider picker and per-provider setup here.</p>
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
