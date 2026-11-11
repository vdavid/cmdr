<script lang="ts">
    import SectionCard from '#lib/ui/SectionCard.svelte'
    import CopyBox from '#lib/ui/CopyBox.svelte'
    import CopyButton from '#lib/ui/CopyButton.svelte'

    const shortCommand = 'sudo killall ptpcamerad'
    const longCommand =
        'sudo /usr/libexec/PlistBuddy -c "Delete :rootless" /System/Library/Frameworks/IOKit.framework/Versions/A/Resources/Info.plist'
    /* Not a command: the box also carries a path (what `AlertDialog` shows), and a
       caller that shortens a long one for display still copies the whole string. */
    const path = '/Volumes/Naspolya/media/photos/2026/07-summer-archive/DSC09241_edited_final_v3_reallyfinal.arw'
    const shortenedPath = '/Volumes/Naspolya/media/…/DSC09241_edited_final_v3_reallyfinal.arw'
</script>

<SectionCard id="components-copybox" label="CopyBox">
    <div class="stack">
        <CopyBox text={shortCommand} />
        <CopyBox text={longCommand} />
        <CopyBox text={path} />
        <CopyBox text={path} displayText={shortenedPath} />
        <!-- The quiet one: for text the line beside it only summarizes (the search
             coverage note's folder lists). Press it for the check. -->
        <p class="quiet">
            Two folders nothing can open
            <CopyButton text={`${path}\n${shortCommand}`} label="Copy the full list of folders" />
        </p>
    </div>
</SectionCard>

<style>
    .stack {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
    }

    .quiet {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }
</style>
