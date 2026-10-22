import { RuleTester } from 'eslint'
import * as svelteParser from 'svelte-eslint-parser'
import rule from './dialog-inside-boundary.js'

// Flat-config RuleTester (ESLint 9+) with the Svelte parser, since the rule
// visits Svelte template AST nodes. RuleTester auto-detects Vitest's
// `describe`/`it` globals, so `run` is called at the top level.
const ruleTester = new RuleTester({
  languageOptions: { parser: svelteParser, ecmaVersion: 'latest', sourceType: 'module' },
})

const routeFile = '/repo/apps/desktop/src/routes/(main)/+page.svelte'

ruleTester.run('dialog-inside-boundary', rule, {
  valid: [
    // The happy path: the boundary wraps the `{#if}` that mounts the dialog.
    {
      code: `<DialogBoundary name="goToPath" onClose={close}>
  {#if show}
    <GoToPathDialog onClose={close} />
  {/if}
</DialogBoundary>`,
      filename: routeFile,
    },
    // Elements, `{#key}`, `{#each}`, and a snippet defined inside the boundary sit between.
    {
      code: `<DialogBoundary name="viewer" onClose={close}>
  <div class="layer">
    {#key id}
      {#each items as item (item.id)}
        {#snippet body()}
          <ViewerCopyDialogs {item} />
        {/snippet}
        {@render body()}
      {/each}
    {/key}
  </div>
</DialogBoundary>`,
      filename: routeFile,
    },
    // Dotted names resolve by their last segment, and the boundary counts as an ancestor.
    {
      code: `<DialogBoundary name="about" onClose={close}><Ui.AboutWindow /></DialogBoundary>`,
      filename: routeFile,
    },
    // Windows-style separators still scope correctly.
    {
      code: `<DialogBoundary name="x" onClose={close}><CommandPalette /></DialogBoundary>`,
      filename: 'C:\\repo\\apps\\desktop\\src\\routes\\+page.svelte',
    },
    // Outside `src/routes/`: dialogs compose dialogs freely (the route owns the boundary).
    {
      code: `{#if show}<ConfirmDialog />{/if}`,
      filename: '/repo/apps/desktop/src/lib/file-operations/TransferDialog.svelte',
    },
    // The dev-only component catalog mounts dialogs for display.
    {
      code: `<ConfirmDialog /><CommandPalette />`,
      filename: '/repo/apps/desktop/src/routes/dev/components/+page.svelte',
    },
    // A route file that IS a dialog: its parent wraps it, so its own dialogs are covered.
    {
      code: `{#if open}<ModalDialog />{/if}`,
      filename: '/repo/apps/desktop/src/routes/viewer/ViewerCopyDialogs.svelte',
    },
    // Non-dialog components, the boundary itself, and a name that only starts with `Dialog`.
    {
      code: `<FilePane /><DialogBoundary name="x"><DialogGallery /></DialogBoundary><DialogGallery />`,
      filename: routeFile,
    },
    // Plain HTML elements aren't components.
    {
      code: `<dialog open>x</dialog>`,
      filename: routeFile,
    },
  ],
  invalid: [
    // A bare `{#if}` in a route: a render throw blanks the window.
    {
      code: `{#if show}<GoToPathDialog />{/if}`,
      filename: routeFile,
      errors: [{ messageId: 'missingBoundary', data: { name: 'GoToPathDialog' } }],
    },
    // Some other wrapper component isn't a boundary.
    {
      code: `<Wrapper>{#if show}<ConfirmDialog />{/if}</Wrapper>`,
      filename: routeFile,
      errors: [{ messageId: 'missingBoundary', data: { name: 'ConfirmDialog' } }],
    },
    // Every dialog-ish suffix counts.
    {
      code: `<AboutWindow />
<CommandPalette />
<SignInSheet />
<SourcesGonePrompt />
<ViewerCopyDialogs />
<OnboardingWizard />
<ExpirationModal />
<ModalDialog />`,
      filename: routeFile,
      errors: [
        { messageId: 'missingBoundary', data: { name: 'AboutWindow' } },
        { messageId: 'missingBoundary', data: { name: 'CommandPalette' } },
        { messageId: 'missingBoundary', data: { name: 'SignInSheet' } },
        { messageId: 'missingBoundary', data: { name: 'SourcesGonePrompt' } },
        { messageId: 'missingBoundary', data: { name: 'ViewerCopyDialogs' } },
        { messageId: 'missingBoundary', data: { name: 'OnboardingWizard' } },
        { messageId: 'missingBoundary', data: { name: 'ExpirationModal' } },
        { messageId: 'missingBoundary', data: { name: 'ModalDialog' } },
      ],
    },
    // Dotted name, unwrapped.
    {
      code: `<Ui.AboutWindow />`,
      filename: routeFile,
      errors: [{ messageId: 'missingBoundary', data: { name: 'AboutWindow' } }],
    },
    // A sibling boundary doesn't cover the dialog: it must be an ancestor.
    {
      code: `<DialogBoundary name="x" onClose={close}></DialogBoundary>
{#if show}<ConfirmDialog />{/if}`,
      filename: routeFile,
      errors: [{ messageId: 'missingBoundary', data: { name: 'ConfirmDialog' } }],
    },
    // A top-level snippet isn't inside a boundary, even if rendered inside one.
    {
      code: `{#snippet dialogs()}<ConfirmDialog />{/snippet}
<DialogBoundary name="x" onClose={close}>{@render dialogs()}</DialogBoundary>`,
      filename: routeFile,
      errors: [{ messageId: 'missingBoundary', data: { name: 'ConfirmDialog' } }],
    },
  ],
})
