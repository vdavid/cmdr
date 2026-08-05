<!--
  The tier 2 fixture for i18n key folding in `.svelte`: every shape the feature handles, in every place a `.svelte`
  file can put it. Keys resolve against `apps/desktop/src/lib/intl/messages/en/sandbox.json` in this same fixture
  project, the same catalog `sample.ts` reads.
-->
<script lang="ts">
  import Trans from './Trans.svelte'

  let { provider }: { provider: string } = $props()

  // Inside the `<script>` block: ordinary TypeScript, lazily parsed under the one `SvelteHTML` root.
  const title = tString('crashReporter.dialog.title')

  const providerSetting = {
    labelKey: 'settings.ai.provider.label',
    descriptionKey: 'settings.ai.provider.description',
  }

  // Left alone on purpose: a key the catalog doesn't have, and a key built by template.
  const missing = tString('crashReporter.dialog.renamedAway')
  const templated = getMessage(`errors.${provider}`)
</script>

<h1>{title}</h1>

<!-- In the template: the same JavaScript PSI, reached by walking down from the `SvelteHTML` root. -->
<p>{tString('crashReporter.dialog.body')}</p>

<!-- Not JavaScript at all: an ordinary XML attribute in the same root. -->
<Trans key="ui.loadingIcon.cancelHint" />

<!--
  Inside a `{#snippet}` block, which is a Svelte 5 shape the earlier PSI spike never covered. This is where the
  regression was first spotted in David's own IDE, so the fixture keeps it in the picture.
-->
{#snippet progress()}
  <span>{getMessage('transfer.progress.copying')}</span>
  <Trans key="ui.loadingIcon.cancelHint" />
{/snippet}

{#each [providerSetting] as setting}
  <label>{setting.labelKey}</label>
{/each}
