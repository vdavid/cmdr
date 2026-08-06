<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import CommandBox from '$lib/ui/CommandBox.svelte'

    const shortCommand = 'sudo killall ptpcamerad'
    const longCommand =
        'sudo /usr/libexec/PlistBuddy -c "Delete :rootless" /System/Library/Frameworks/IOKit.framework/Versions/A/Resources/Info.plist'
    /* Not a command: the box also carries a path (what `AlertDialog` shows), and a
       caller that shortens a long one for display still copies the whole string. */
    const path = '/Volumes/Naspolya/media/photos/2026/07-summer-archive/DSC09241_edited_final_v3_reallyfinal.arw'
    const shortenedPath = '/Volumes/Naspolya/media/…/DSC09241_edited_final_v3_reallyfinal.arw'
</script>

<SectionCard id="components-commandbox" label="CommandBox">
    <div class="stack">
        <CommandBox command={shortCommand} />
        <CommandBox command={longCommand} />
        <CommandBox command={path} />
        <CommandBox command={path} displayText={shortenedPath} />
    </div>
</SectionCard>

<style>
    .stack {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
    }
</style>
