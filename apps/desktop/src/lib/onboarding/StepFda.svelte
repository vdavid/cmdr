<script lang="ts">
    import OnboardingStepShell from './OnboardingStepShell.svelte'

    /**
     * M1 placeholder. M2 ports the Full Disk Access copy + actions from
     * `FullDiskAccessPrompt.svelte` and adds the three variants (first-ask, revoked,
     * already-granted). See `docs/specs/onboarding-revamp-plan.md` § M2.
     */
</script>

<OnboardingStepShell>
    <h2 class="step-title">Step 1 — Full disk access</h2>
    <p class="step-placeholder">M2 ports the FDA copy and actions here.</p>
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
