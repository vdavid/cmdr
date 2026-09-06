<script lang="ts">
    import type { SettingId } from '$lib/settings'
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import { useBooleanSetting } from './boolean-setting.svelte'

    interface Props {
        id: SettingId
        disabled?: boolean
    }

    const { id, disabled = false }: Props = $props()
    const setting = useBooleanSetting(id)
</script>

<Checkbox checked={setting.checked} {disabled} ariaLabel={setting.label} onCheckedChange={setting.set} />
