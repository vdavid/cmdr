<script lang="ts">
    /**
     * One renderer per `SignInShape` variant: exactly what the backend said to
     * ask, and nothing else.
     *
     * ❗ **Username editability is a property of the VARIANT, not of the sheet's
     * mode.** `password` and `key_passphrase` render the account as a read-only
     * header, because SFTP's and WebDAV's `reconnect_with_credentials` refuse a
     * changed username: the volume id IS the account. `username_password` renders
     * it editable, because SMB's accepts a new one and rewrites its params, which
     * is how re-auth-as-someone-else works and must keep working. Reading either
     * rule as a MODE rule breaks the other protocol.
     *
     * ❗ `nothing` never reaches here: a shape with nothing to ask means the sheet
     * never opens (`SignInSheet.svelte`).
     */
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import RadioGroup, { type RadioItem } from '$lib/ui/RadioGroup.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import type { SignInShape } from '$lib/ipc/bindings'

    interface Props {
        shape: SignInShape
        /** The account the server already knows, for the read-only variants. */
        accountLabel: string
        /** Editable only where the variant says so. */
        username: string
        secret: string
        remember: boolean
        /** Guest, for `username_password` with `guestAllowed`. */
        guest: boolean
        disabled: boolean
        /** The sentence under the secret field, when the last attempt was refused. */
        secretRefusal?: string
        onChange: (patch: { username?: string; secret?: string; remember?: boolean; guest?: boolean }) => void
        /** The secret input, so the sheet can put focus back on it after a refusal. */
        secretInput?: HTMLInputElement
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        shape,
        accountLabel,
        username,
        secret,
        remember,
        guest,
        disabled,
        secretRefusal,
        onChange,
        secretInput = $bindable(),
    }: Props = $props()

    /** Whether the account is a field or a header. The VARIANT decides, not the mode. */
    const usernameIsEditable = $derived(shape.kind === 'username_password')
    const guestAllowed = $derived(shape.kind === 'username_password' && shape.guestAllowed)

    /**
     * The same rule the SMB form has always used: offer guest where the share
     * allows it, and start there, because a share that lets anyone in usually
     * means to.
     */
    const guestItems: RadioItem[] = $derived([
        { value: 'guest', label: tString('servers.sheet.connectAsGuest') },
        { value: 'credentials', label: tString('servers.sheet.signInWithCredentials') },
    ])

    const secretLabel = $derived(
        shape.kind === 'key_passphrase' ? tString('servers.sheet.passphrase') : tString('servers.sheet.password'),
    )
    /** A passphrase is not the account's password, and autofill must not offer one. */
    const secretAutocomplete = $derived(shape.kind === 'key_passphrase' ? 'off' : 'current-password')
    const fieldsDisabled = $derived(disabled || guest)
</script>

{#if guestAllowed}
    <div class="field">
        <RadioGroup
            items={guestItems}
            value={guest ? 'guest' : 'credentials'}
            onValueChange={(v: string) => {
                onChange({ guest: v === 'guest' })
            }}
            {disabled}
            ariaLabel={tString('servers.sheet.connectionModeLegend')}
        />
    </div>
{/if}

<div class="field">
    <label for="sign-in-username" class="field-label">{tString('servers.sheet.username')}</label>
    {#if usernameIsEditable}
        <TextInput
            id="sign-in-username"
            value={username}
            placeholder={tString('servers.sheet.usernamePlaceholder')}
            oninput={(e: Event) => {
                onChange({ username: (e.currentTarget as HTMLInputElement).value })
            }}
            disabled={fieldsDisabled}
            autocomplete="username"
            autocapitalize="off"
            spellcheck={false}
        />
    {:else}
        <!-- Read-only because the volume id IS this account: a changed username
             would name a different server, and the backend refuses one. -->
        <p id="sign-in-username" class="account-header">{accountLabel}</p>
    {/if}
</div>

<div class="field">
    <label for="sign-in-secret" class="field-label">{secretLabel}</label>
    <TextInput
        id="sign-in-secret"
        bind:inputElement={secretInput}
        type="password"
        value={secret}
        oninput={(e: Event) => {
            onChange({ secret: (e.currentTarget as HTMLInputElement).value })
        }}
        disabled={fieldsDisabled}
        invalid={secretRefusal !== undefined}
        aria-describedby={secretRefusal ? 'sign-in-secret-refusal' : undefined}
        autocomplete={secretAutocomplete}
    />
    {#if secretRefusal}
        <p id="sign-in-secret-refusal" class="field-refusal" role="alert">{secretRefusal}</p>
    {/if}
</div>

<div class="field">
    <Checkbox
        checked={remember}
        onCheckedChange={(checked: boolean) => {
            onChange({ remember: checked })
        }}
        disabled={fieldsDisabled}
    >
        {tString('servers.sheet.remember')}
    </Checkbox>
</div>

<style>
    .field {
        margin-bottom: var(--spacing-md);
    }

    .field-label {
        display: block;
        margin-bottom: var(--spacing-xs);
        font-size: var(--font-size-sm);
        font-weight: 500;
        color: var(--color-text-secondary);
    }

    .account-header {
        margin: 0;
        padding: var(--spacing-xs) 0;
        color: var(--color-text-primary);
    }

    .field-refusal {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-error-text);
    }
</style>
