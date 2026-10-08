<script lang="ts">
    import type { SettingId } from '$lib/settings'
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import { useBooleanSetting } from './boolean-setting.svelte'
    import { useSettingLock } from './setting-lock.svelte'

    interface Props {
        id: SettingId
        disabled?: boolean
    }

    const { id, disabled = false }: Props = $props()
    const setting = useBooleanSetting(id)
    const lock = useSettingLock(id)
</script>

<Checkbox
    checked={setting.checked}
    disabled={disabled || lock.locked}
    ariaLabel={setting.label}
    ariaDescribedBy={lock.describedBy()}
    onCheckedChange={setting.set}
/>
