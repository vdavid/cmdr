<script lang="ts">
    /**
     * The control of the "Edit files in" row, in the Text editor card of
     * `NavigationAndFileOpsSection.svelte`.
     *
     * The list, the "Checking…" state, the picker, and the refresh on every write
     * are `AppChoiceSelect.svelte`'s; this row brings `list_text_editors`, its
     * options, and one step of its own: a "Choose an app…" pick is stored the way
     * Rust canonicalizes it, a bundle id unless the user picked a different copy
     * than the one macOS would launch (`src-tauri/src/file_system/DETAILS.md` §
     * Text editor).
     *
     * Rationale: `DETAILS.md` § "Edit files in".
     */
    import { tString } from '$lib/intl/messages.svelte'
    import { getUiLocale } from '$lib/intl/locale'
    import { listTextEditors } from '$lib/tauri-commands'
    import type { TextEditorList } from '$lib/ipc/bindings'
    import AppChoiceSelect from './AppChoiceSelect.svelte'
    import type { AppChoiceOption } from './app-choice-options'
    import { selectedTextEditorId, textEditorItems } from './text-editor-options'

    interface Props {
        /** The row's accessible name, from the registry label. */
        ariaLabel: string
    }

    const { ariaLabel }: Props = $props()

    function systemDefaultLabel(appName: string | null): string {
        return appName === null
            ? tString('settings.behavior.textEditorApp.systemDefaultUnnamed')
            : tString('settings.behavior.textEditorApp.systemDefault', { app: appName })
    }

    function editorItems(list: TextEditorList): AppChoiceOption[] {
        return textEditorItems(list, {
            systemDefault: systemDefaultLabel,
            chooseApp: tString('settings.behavior.textEditorApp.chooseApp'),
            locale: getUiLocale(),
        })
    }

    /**
     * The value Rust would store for this pick. A timed-out answer knows no
     * canonical form, so the path itself goes in, which `open -a` launches as is.
     */
    async function canonicalPick(picked: string): Promise<string> {
        const answer = await listTextEditors(picked)
        return answer.data.chosenId ?? picked
    }
</script>

<AppChoiceSelect
    settingId="behavior.textEditorApp"
    {ariaLabel}
    checkingLabel={tString('settings.behavior.textEditorApp.checking')}
    pickerTitle={tString('settings.behavior.textEditorApp.chooseAppTitle')}
    listApps={listTextEditors}
    itemsFor={editorItems}
    selectedIn={selectedTextEditorId}
    resolvePick={canonicalPick}
/>
