<script lang="ts">
    /**
     * The control of the "Open terminal here uses" row, in the Terminal card of
     * `NavigationAndFileOpsSection.svelte`.
     *
     * It can't be `SettingSelect`: those options are registry constants, and these
     * are whatever terminal apps this Mac has right now. `list_terminal_apps` answers
     * that in a few LaunchServices lookups, so the row asks on mount and again after
     * every write instead of caching a list that goes stale the moment someone
     * installs Ghostty. There's deliberately no refresh button.
     *
     * The last row, "Choose an app…", stores an absolute `.app` path rather than a
     * bundle id; Rust's `parse_choice` tells the two apart structurally. Cancelling
     * the picker leaves the setting alone.
     *
     * Rationale, and why the app list lives in Rust: `DETAILS.md` § "Open terminal
     * here", and `src-tauri/src/file_system/DETAILS.md`.
     */
    import { onMount } from 'svelte'
    import { open as openAppPicker } from '@tauri-apps/plugin-dialog'
    import Select from '$lib/ui/Select.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { getSetting, setSetting, onSpecificSettingChange } from '$lib/settings'
    import { listTerminalApps } from '$lib/tauri-commands'
    import { getAppLogger } from '$lib/logging/logger'
    import type { TerminalAppList } from '$lib/ipc/bindings'
    import { CHOOSE_APP_VALUE, selectedTerminalAppId, terminalAppItems } from './terminal-app-options'

    interface Props {
        /** The row's accessible name, from the registry label. */
        ariaLabel: string
    }

    const { ariaLabel }: Props = $props()

    const SETTING_ID = 'behavior.openTerminalHereApp'
    const log = getAppLogger('settings')

    // Empty until the first answer lands, which is what puts the control in its
    // disabled "Checking…" state. A timed-out query answers empty too, so the
    // control stays disabled rather than claiming a shorter list than reality.
    let list = $state<TerminalAppList>({ apps: [], chosenId: null })

    async function refresh(): Promise<void> {
        try {
            const answer = await listTerminalApps(getSetting(SETTING_ID))
            list = answer.data
        } catch (err) {
            log.warn('Listing the terminal apps did not work: {err}', { err: String(err) })
        }
    }

    onMount(() => {
        void refresh()
        // Another window can move the choice, and so can the action's
        // uninstalled-app fallback. Re-ask so the row keeps telling the truth.
        return onSpecificSettingChange(SETTING_ID, () => void refresh())
    })

    const items = $derived(terminalAppItems(list.apps, tString('settings.behavior.openTerminalHereApp.chooseApp')))
    const value = $derived(selectedTerminalAppId(list))
    const ready = $derived(list.apps.length > 0)

    async function chooseApp(): Promise<void> {
        let picked: string | string[] | null
        try {
            picked = await openAppPicker({
                multiple: false,
                directory: false,
                defaultPath: '/Applications',
                title: tString('settings.behavior.openTerminalHereApp.chooseAppTitle'),
                filters: [{ name: 'Applications', extensions: ['app'] }],
            })
        } catch (err) {
            log.warn('The app picker did not open: {err}', { err: String(err) })
            return
        }
        // The user cancelled, or picked something that isn't one path.
        if (typeof picked !== 'string') return
        setSetting(SETTING_ID, picked)
        // The pick isn't in `apps` yet, and only the backend knows its name and icon.
        await refresh()
    }

    function handleChange(next: string): void {
        if (next === CHOOSE_APP_VALUE) {
            void chooseApp()
            return
        }
        // Move the shown row now; the write's own change event re-asks right after.
        list = { ...list, chosenId: next }
        setSetting(SETTING_ID, next)
    }
</script>

<Select
    items={ready ? items : []}
    value={ready ? value : ''}
    onChange={handleChange}
    placeholder={tString('settings.behavior.openTerminalHereApp.checking')}
    disabled={!ready}
    {ariaLabel}
    portal
/>
