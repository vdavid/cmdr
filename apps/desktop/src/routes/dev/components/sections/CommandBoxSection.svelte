<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import CommandBox from '$lib/ui/CommandBox.svelte'

    const shortCommand = 'sudo killall ptpcamerad'
    const longCommand =
        'sudo /usr/libexec/PlistBuddy -c "Delete :rootless" /System/Library/Frameworks/IOKit.framework/Versions/A/Resources/Info.plist'
</script>

<SectionCard id="components-commandbox" label="CommandBox">
    <div class="stack">
        <CommandBox command={shortCommand} />
        <CommandBox command={longCommand} />
    </div>
</SectionCard>

<style>
    .stack {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
    }
</style>
