<script lang="ts">
    /**
     * The control of the "Open terminal here uses" row, in the Terminal card of
     * `NavigationAndFileOpsSection.svelte`.
     *
     * The list, the "Checking…" state, the picker, and the refresh on every write
     * are `AppChoiceSelect.svelte`'s; this row brings the terminal list call and
     * its options. A "Choose an app…" pick stores the absolute `.app` path as
     * picked, and Rust's `parse_choice` tells it apart from a bundle id
     * structurally.
     *
     * Rationale, and why the app list lives in Rust: `DETAILS.md` § "Open terminal
     * here", and `src-tauri/src/file_system/DETAILS.md`.
     */
    import { tString } from '$lib/intl/messages.svelte'
    import { listTerminalApps } from '$lib/tauri-commands'
    import type { TerminalAppList } from '$lib/ipc/bindings'
    import AppChoiceSelect from './AppChoiceSelect.svelte'
    import type { AppChoiceOption } from './app-choice-options'
    import { selectedTerminalAppId, terminalAppItems } from './terminal-app-options'

    interface Props {
        /** The row's accessible name, from the registry label. */
        ariaLabel: string
    }

    const { ariaLabel }: Props = $props()

    function terminalItems(list: TerminalAppList): AppChoiceOption[] {
        return terminalAppItems(list.apps, tString('settings.behavior.openTerminalHereApp.chooseApp'))
    }
</script>

<AppChoiceSelect
    settingId="behavior.openTerminalHereApp"
    {ariaLabel}
    checkingLabel={tString('settings.behavior.openTerminalHereApp.checking')}
    pickerTitle={tString('settings.behavior.openTerminalHereApp.chooseAppTitle')}
    listApps={listTerminalApps}
    itemsFor={terminalItems}
    selectedIn={selectedTerminalAppId}
/>
