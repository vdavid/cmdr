<script lang="ts">
    /**
     * Where an S3 account lives: the provider, the one field that preset takes,
     * and the bucket. It stands in for the address field when S3 is selected.
     *
     * ❗ **The preset decides the endpoint** (`s3-form.ts`), so only "Other
     * S3-compatible" asks for a URL. AWS, B2, and Wasabi take a region, R2 an
     * account ID, Hetzner a location and Spaces a region from their own lists,
     * and GCS nothing at all (one global endpoint), just a line on its keys.
     *
     * ❗ **Two refusal slots, one input for a preset.** `address` is the field
     * that makes the endpoint (the Other URL, else the preset's own field) and
     * `region` is the region; for a preset they're the same input, so either
     * sentence lands under it. Other keeps them apart.
     */
    import Button from '$lib/ui/Button.svelte'
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import Select, { type SelectItem } from '$lib/ui/Select.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import type { MessageKey } from '$lib/intl/keys.gen'
    import {
        HETZNER_LOCATIONS,
        S3_PROVIDERS,
        SPACES_REGIONS,
        type S3FormFields,
        type S3ProviderKind,
    } from './s3-form'

    const PROVIDER_LABEL_KEY: Record<S3ProviderKind, MessageKey> = {
        aws: 'servers.sheet.s3ProviderAws',
        r2: 'servers.sheet.s3ProviderR2',
        b2: 'servers.sheet.s3ProviderB2',
        wasabi: 'servers.sheet.s3ProviderWasabi',
        hetzner: 'servers.sheet.s3ProviderHetzner',
        gcs: 'servers.sheet.s3ProviderGcs',
        digitalocean: 'servers.sheet.s3ProviderDigitalOcean',
        other: 'servers.sheet.s3ProviderOther',
    }

    /** A region each region-taking preset really has, for the placeholder. Codes, so never translated. */
    const REGION_EXAMPLE: Partial<Record<S3ProviderKind, string>> = {
        aws: 'eu-west-1',
        b2: 'us-west-004',
        wasabi: 'eu-central-1',
    }

    interface Props {
        fields: S3FormFields
        disabled: boolean
        /** ❗ Off in edit mode: the provider, its field, and the bucket name the place. */
        identityEditable: boolean
        /** Off for an ACCOUNT's edit: the account is every bucket, so no one bucket is its. */
        showBucket?: boolean
        /** The sentence under the field that makes the endpoint. */
        addressRefusal?: string
        /** The sentence under the region. */
        regionRefusal?: string
        /** The sentence under the bucket. */
        bucketRefusal?: string
        /** Offered on `region_mismatch` when the server named the bucket's region: switches to it and tries again. */
        onUseRegion?: () => void
        /** The region `onUseRegion` switches to, for its label. */
        suggestedRegion?: string
        onChange: (patch: Partial<S3FormFields>) => void
        /** The input that makes the endpoint (the Other URL, else a preset's field), for focus after a refusal. */
        addressInput?: HTMLInputElement
        /** Other's region input. A preset's region is `addressInput`. */
        regionInput?: HTMLInputElement
        bucketInput?: HTMLInputElement
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        fields,
        disabled,
        identityEditable,
        showBucket = true,
        addressRefusal,
        regionRefusal,
        bucketRefusal,
        onUseRegion,
        suggestedRegion,
        onChange,
        addressInput = $bindable(),
        regionInput = $bindable(),
        bucketInput = $bindable(),
    }: Props = $props()

    const providerItems: SelectItem[] = $derived(
        S3_PROVIDERS.map((provider) => ({ value: provider, label: tString(PROVIDER_LABEL_KEY[provider]) })),
    )
    const locationItems: SelectItem[] = HETZNER_LOCATIONS.map((location) => ({ value: location, label: location }))
    const spacesRegionItems: SelectItem[] = SPACES_REGIONS.map((region) => ({ value: region, label: region }))

    const identityDisabled = $derived(disabled || !identityEditable)
    /** A preset's one field carries both sentences; Other splits them. */
    const presetRefusal = $derived(addressRefusal ?? regionRefusal)
    const bucketDescribedBy = $derived(
        [identityEditable ? 'server-s3-bucket-help' : null, bucketRefusal ? 'server-s3-bucket-refusal' : null]
            .filter((id) => id !== null)
            .join(' ') || undefined,
    )
    const takesRegion = $derived(fields.provider === 'aws' || fields.provider === 'b2' || fields.provider === 'wasabi')
    const regionPlaceholder = $derived.by(() => {
        const example = REGION_EXAMPLE[fields.provider]
        return example === undefined ? undefined : tString('servers.sheet.examplePlaceholder', { example })
    })
</script>

<!-- eslint-disable @typescript-eslint/no-confusing-void-expression -- Svelte {@render} syntax: a snippet call is a void expression by design -->

{#snippet useRegionRemedy()}
    {#if onUseRegion && suggestedRegion}
        <div class="remedy-row">
            <Button size="mini" onclick={onUseRegion} {disabled}>
                {tString('servers.sheet.s3UseRegion', { region: suggestedRegion })}
            </Button>
        </div>
    {/if}
{/snippet}

{#snippet zoneRefusal(text: string | undefined)}
    {#if text}
        <p id="server-s3-zone-refusal" class="field-refusal" role="alert">{text}</p>
        {@render useRegionRemedy()}
    {/if}
{/snippet}

<div class="field">
    <span id="server-s3-provider-label" class="field-label">{tString('servers.sheet.s3Provider')}</span>
    <Select
        items={providerItems}
        value={fields.provider}
        onChange={(value: string) => {
            onChange({ provider: value as S3ProviderKind })
        }}
        disabled={identityDisabled}
        ariaLabel={tString('servers.sheet.s3Provider')}
    />
</div>

{#if takesRegion}
    <div class="field">
        <label for="server-s3-region" class="field-label">{tString('servers.sheet.s3Region')}</label>
        <TextInput
            id="server-s3-region"
            bind:inputElement={addressInput}
            value={fields.region}
            oninput={(e: Event) => {
                onChange({ region: (e.currentTarget as HTMLInputElement).value })
            }}
            disabled={identityDisabled}
            invalid={presetRefusal !== undefined}
            aria-describedby={presetRefusal ? 'server-s3-zone-refusal' : undefined}
            placeholder={regionPlaceholder}
            autocapitalize="off"
            autocomplete="off"
            spellcheck={false}
        />
        {@render zoneRefusal(presetRefusal)}
    </div>
{:else if fields.provider === 'r2'}
    <div class="field">
        <label for="server-s3-account-id" class="field-label">{tString('servers.sheet.s3AccountId')}</label>
        <TextInput
            id="server-s3-account-id"
            bind:inputElement={addressInput}
            value={fields.accountId}
            oninput={(e: Event) => {
                onChange({ accountId: (e.currentTarget as HTMLInputElement).value })
            }}
            disabled={identityDisabled}
            invalid={presetRefusal !== undefined}
            aria-describedby={presetRefusal ? 'server-s3-zone-refusal' : 'server-s3-account-id-help'}
            autocapitalize="off"
            autocomplete="off"
            spellcheck={false}
        />
        {#if presetRefusal}
            {@render zoneRefusal(presetRefusal)}
        {:else}
            <p id="server-s3-account-id-help" class="field-help">{tString('servers.sheet.s3AccountIdHelp')}</p>
        {/if}
    </div>
{:else if fields.provider === 'hetzner'}
    <div class="field">
        <span class="field-label">{tString('servers.sheet.s3Location')}</span>
        <Select
            items={locationItems}
            value={fields.location}
            onChange={(value: string) => {
                onChange({ location: value })
            }}
            disabled={identityDisabled}
            ariaLabel={tString('servers.sheet.s3Location')}
        />
        {@render zoneRefusal(presetRefusal)}
    </div>
{:else if fields.provider === 'digitalocean'}
    <div class="field">
        <span class="field-label">{tString('servers.sheet.s3Region')}</span>
        <Select
            items={spacesRegionItems}
            value={fields.spacesRegion}
            onChange={(value: string) => {
                onChange({ spacesRegion: value })
            }}
            disabled={identityDisabled}
            ariaLabel={tString('servers.sheet.s3Region')}
        />
        {@render zoneRefusal(presetRefusal)}
    </div>
{:else if fields.provider === 'gcs'}
    <div class="field">
        <p class="field-help">{tString('servers.sheet.s3GcsKeyHelp')}</p>
        {@render zoneRefusal(presetRefusal)}
    </div>
{:else}
    <div class="field">
        <label for="server-s3-endpoint" class="field-label">{tString('servers.sheet.s3Endpoint')}</label>
        <TextInput
            id="server-s3-endpoint"
            bind:inputElement={addressInput}
            value={fields.endpoint}
            oninput={(e: Event) => {
                onChange({ endpoint: (e.currentTarget as HTMLInputElement).value })
            }}
            disabled={identityDisabled}
            invalid={addressRefusal !== undefined}
            aria-describedby={addressRefusal ? 'server-s3-zone-refusal' : undefined}
            placeholder={tString('servers.sheet.examplePlaceholder', { example: 'https://s3.example.com' })}
            autocapitalize="off"
            autocomplete="off"
            spellcheck={false}
        />
        {@render zoneRefusal(addressRefusal)}
    </div>

    <div class="field">
        <label for="server-s3-region" class="field-label">{tString('servers.sheet.s3Region')}</label>
        <TextInput
            id="server-s3-region"
            bind:inputElement={regionInput}
            value={fields.region}
            oninput={(e: Event) => {
                onChange({ region: (e.currentTarget as HTMLInputElement).value })
            }}
            disabled={identityDisabled}
            invalid={regionRefusal !== undefined}
            aria-describedby={regionRefusal ? 'server-s3-region-refusal' : undefined}
            placeholder={tString('servers.sheet.s3RegionOptionalPlaceholder')}
            autocapitalize="off"
            autocomplete="off"
            spellcheck={false}
        />
        {#if regionRefusal}
            <p id="server-s3-region-refusal" class="field-refusal" role="alert">{regionRefusal}</p>
            {@render useRegionRemedy()}
        {/if}
    </div>

    <div class="field">
        <Checkbox
            checked={fields.pathStyle}
            onCheckedChange={(checked: boolean) => {
                onChange({ pathStyle: checked })
            }}
            disabled={identityDisabled}
        >
            {tString('servers.sheet.s3PathStyle')}
        </Checkbox>
        <p class="field-help">{tString('servers.sheet.s3PathStyleHelp')}</p>
    </div>
{/if}

{#if showBucket}
    <div class="field">
        <label for="server-s3-bucket" class="field-label">{tString('servers.sheet.s3Bucket')}</label>
        <TextInput
            id="server-s3-bucket"
            bind:inputElement={bucketInput}
            value={fields.bucket}
            oninput={(e: Event) => {
                onChange({ bucket: (e.currentTarget as HTMLInputElement).value })
            }}
            disabled={identityDisabled}
            invalid={bucketRefusal !== undefined}
            aria-describedby={bucketDescribedBy}
            autocapitalize="off"
            autocomplete="off"
            spellcheck={false}
        />
        <!-- ❗ No "Optional" placeholder: a key limited to one bucket can't open the account
             root, so for that key the field is required, and the help line says when. It
             stays on screen beside a refusal, which reads as the answer to it. Edit mode
             locks the bucket, so a line about what to type there would be inert. -->
        {#if identityEditable}
            <p id="server-s3-bucket-help" class="field-help">{tString('servers.sheet.s3BucketHelp')}</p>
        {/if}
        {#if bucketRefusal}
            <p id="server-s3-bucket-refusal" class="field-refusal" role="alert">{bucketRefusal}</p>
        {/if}
    </div>
{/if}
<!-- eslint-enable @typescript-eslint/no-confusing-void-expression -->

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

    .field-help {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    .field-refusal {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-error-text);
    }

    .remedy-row {
        margin-top: var(--spacing-sm);
    }
</style>
