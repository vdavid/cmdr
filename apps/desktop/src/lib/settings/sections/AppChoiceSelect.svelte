<script lang="ts" generics="L">
    /**
     * The control behind every Settings row that picks an app off this Mac: the
     * "Edit files in" row and the "Open terminal here uses" row. Each row is a thin
     * wrapper that hands this shell its list call, its option builders, and its
     * strings.
     *
     * It can't be `SettingSelect`: those options are registry constants, and these
     * are whatever apps this Mac has right now. The backend answers that in a few
     * LaunchServices lookups, so the row asks on mount and again after every write
     * of its setting instead of caching a list that goes stale the moment someone
     * installs a new app. There's deliberately no refresh button.
     *
     * The last row, "Choose an app…", opens the app picker and stores what
     * `resolvePick` makes of the picked `.app` path (the path itself when a row
     * passes none). Cancelling the picker leaves the setting alone.
     *
     * Rationale: `DETAILS.md` § "Rows that pick an app".
     */
    import { onMount } from 'svelte'
    import { open as openAppPicker } from '@tauri-apps/plugin-dialog'
    import Select from '$lib/ui/Select.svelte'
    import { getSetting, setSetting, onSpecificSettingChange } from '$lib/settings'
    import { getAppLogger } from '$lib/logging/logger'
    import type { TimedOut } from '$lib/ipc/bindings'
    import { CHOOSE_APP_VALUE, type AppChoiceOption } from './app-choice-options'

    interface Props {
        /** The setting the row reads and writes. */
        settingId: 'behavior.openTerminalHereApp' | 'behavior.textEditorApp'
        /** The row's accessible name, from the registry label. */
        ariaLabel: string
        /** The disabled control's text until a usable answer lands. */
        checkingLabel: string
        /** The app picker window's title. */
        pickerTitle: string
        /** Asks the backend which apps exist, handing it the stored choice. */
        listApps: (appChoice: string) => Promise<TimedOut<L>>
        /** The dropdown rows for an answer, "Choose an app…" included. */
        itemsFor: (list: L) => AppChoiceOption[]
        /** Which row reads as selected for an answer. */
        selectedIn: (list: L) => string
        /** Turns a picked `.app` path into the value to store. Absent means store the path. */
        resolvePick?: (picked: string) => Promise<string>
    }

    const { settingId, ariaLabel, checkingLabel, pickerTitle, listApps, itemsFor, selectedIn, resolvePick }: Props =
        $props()

    const log = getAppLogger('settings')

    // `null` until the first answer lands.
    let answer = $state<TimedOut<L> | null>(null)
    // A row the user just picked, shown before the backend's next answer confirms
    // it, so the control doesn't flick back to the old choice for a moment.
    let pendingChoice = $state<string | null>(null)
    // Only the newest request may land: a pick fires one refresh itself and its
    // write fires another, and an older answer arriving last would show a stale list.
    let latestRequest = 0
    // Every write the row starts: a picked row, or a "Choose an app…" pick. A pick
    // waits on the picker and on `resolvePick` (up to its command's 2 s deadline)
    // while the control stays usable, so a row picked meanwhile must win over it.
    let latestChange = 0

    async function refresh(): Promise<void> {
        const request = ++latestRequest
        try {
            const next = await listApps(getSetting(settingId))
            if (request !== latestRequest) return
            answer = next
            pendingChoice = null
        } catch (err) {
            log.warn('Listing the apps for {settingId} did not work: {err}', { settingId, err: String(err) })
        }
    }

    onMount(() => {
        void refresh()
        // Another window can move the choice, and so can the action's missing-app
        // fallback. Re-ask so the row keeps telling the truth.
        return onSpecificSettingChange(settingId, () => void refresh())
    })

    // Ready once an answer lands that didn't time out, ❌ never "once the app list
    // is non-empty": the text editor list leaves out the system default, so a Mac
    // whose only editor is TextEdit answers a complete, empty list. A timed-out
    // answer claims nothing, so the control stays disabled at "Checking…".
    const ready = $derived(answer !== null && !answer.timedOut)
    const items = $derived(answer === null ? [] : itemsFor(answer.data))
    const value = $derived(pendingChoice ?? (answer === null ? '' : selectedIn(answer.data)))

    async function chooseApp(): Promise<void> {
        const change = ++latestChange
        let picked: string | string[] | null
        try {
            picked = await openAppPicker({
                multiple: false,
                directory: false,
                defaultPath: '/Applications',
                title: pickerTitle,
                filters: [{ name: 'Applications', extensions: ['app'] }],
            })
        } catch (err) {
            log.warn('The app picker did not open: {err}', { err: String(err) })
            return
        }
        // The user cancelled, or picked something that isn't one path.
        if (typeof picked !== 'string') return
        let choice = picked
        if (resolvePick !== undefined) {
            try {
                choice = await resolvePick(picked)
            } catch (err) {
                // The path itself is still a working choice; Rust launches it with `open -a`.
                log.warn('Resolving the picked app did not work, storing its path: {err}', { err: String(err) })
            }
        }
        // A row picked while this pick waited is the newer choice; storing this one now
        // would overwrite it.
        if (change !== latestChange) return
        setSetting(settingId, choice)
        // The pick may not be in the list yet, and only the backend knows its name and icon.
        await refresh()
    }

    function handleChange(next: string): void {
        if (next === CHOOSE_APP_VALUE) {
            void chooseApp()
            return
        }
        latestChange++
        // Move the shown row now; the write's own change event re-asks right after.
        pendingChoice = next
        setSetting(settingId, next)
    }
</script>

<Select
    items={ready ? items : []}
    value={ready ? value : ''}
    onChange={handleChange}
    placeholder={checkingLabel}
    disabled={!ready}
    {ariaLabel}
/>
