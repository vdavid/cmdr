import { RuleTester } from 'eslint'
import * as svelteParser from 'svelte-eslint-parser'
import rule from './no-title-attribute.js'

// Flat-config RuleTester (ESLint 9+) with the Svelte parser, since the rule
// visits Svelte template AST nodes. RuleTester auto-detects Vitest's
// `describe`/`it` globals, so `run` is called at the top level.
const ruleTester = new RuleTester({
  languageOptions: { parser: svelteParser, ecmaVersion: 'latest', sourceType: 'module' },
})

ruleTester.run('no-title-attribute', rule, {
  valid: [
    // The intended path.
    {
      code: `<span use:tooltip={tString('fileExplorer.columns.gitTitle')}>Git</span>`,
      filename: 'src/lib/file-explorer/views/FullListHeader.svelte',
    },
    // A component's `title` is an ordinary prop that happens to share the name.
    // This is the bulk of `title=` in the tree, so it must never be flagged.
    {
      code: `<SettingsSection title={tString('settings.section.git')}><span>x</span></SettingsSection>`,
      filename: 'src/lib/settings/sections/GitSection.svelte',
    },
    {
      code: `<AlertDialog title={props.title} message={props.message} onClose={close} />`,
      filename: 'src/lib/file-explorer/pane/DialogManager.svelte',
    },
    // A namespaced component prop is still a component prop.
    {
      code: `<Dialog.Root title="Catalog preview" />`,
      filename: 'src/routes/dev/components/sections/Dialogs.svelte',
    },
    // Embedded content takes its ACCESSIBLE NAME from `title` (axe's
    // `frame-title`), so the attribute has to stay.
    {
      code: `<embed class="media-pdf" type="application/pdf" {src} title={fileName} aria-label={fileName} />`,
      filename: 'src/routes/viewer/MediaPdfView.svelte',
    },
    {
      code: `<iframe title="Preview" src={url}></iframe>`,
      filename: 'src/lib/whatever/Preview.svelte',
    },
    {
      code: `<object title="Preview" data={url}></object>`,
      filename: 'src/lib/whatever/Preview.svelte',
    },
    // On `<abbr>` / `<dfn>` the title is the term's expansion, which AT reads
    // as part of the word rather than as a hover hint.
    {
      code: `<abbr title="Localization">L10n</abbr>`,
      filename: 'src/lib/whatever/Row.svelte',
    },
    {
      code: `<dfn title="Message Transfer Protocol">MTP</dfn>`,
      filename: 'src/lib/whatever/Row.svelte',
    },
    // An element with no title at all.
    {
      code: `<span class="repo-chip" aria-label={label}>x</span>`,
      filename: 'src/lib/file-explorer/git/RepoChip.svelte',
    },
    // A spread can't be resolved statically, and the component-prop case makes
    // guessing at it too false-positive-prone.
    {
      code: `<span {...attrs}>x</span>`,
      filename: 'src/lib/whatever/Row.svelte',
    },
    // A look-alike attribute name is not `title`.
    {
      code: `<span data-title={label} aria-labelledby="x">y</span>`,
      filename: 'src/lib/whatever/Row.svelte',
    },
    // (The per-element opt-out comment is exercised end-to-end by the real
    // eslint config, not here: RuleTester registers the rule under a
    // `rule-to-test/*` id, so a `cmdr/*` disable directive can't match inside
    // the harness.)
  ],
  invalid: [
    // A static title on a plain element.
    {
      code: `<button type="button" title="Switch theme">x</button>`,
      filename: 'src/lib/whatever/Row.svelte',
      errors: [{ messageId: 'noTitleAttribute', data: { element: 'button' } }],
    },
    // A dynamic title is flagged too: unlike an element's `type`, the VALUE
    // never matters here, only that a native tooltip is being asked for.
    {
      code: `<div class="gauge" data-state={state} title={tooltip}>x</div>`,
      filename: 'src/lib/ask-cmdr/AskCmdrContextGauge.svelte',
      errors: [{ messageId: 'noTitleAttribute', data: { element: 'div' } }],
    },
    // The `{title}` shorthand is the same attribute in a different spelling.
    {
      code: `<span {title}>x</span>`,
      filename: 'src/lib/whatever/Row.svelte',
      errors: [{ messageId: 'noTitleAttribute', data: { element: 'span' } }],
    },
    // A conditional value is still a native title.
    {
      code: `<span class="col-git" aria-label={label} title={status ? labelFor(status) : ''}>x</span>`,
      filename: 'src/lib/file-explorer/views/FullList.svelte',
      errors: [{ messageId: 'noTitleAttribute', data: { element: 'span' } }],
    },
    // Having an `aria-label` doesn't excuse the title: the two do different
    // jobs, and only the title is the redundant native tooltip.
    {
      code: `<span class="tag-dots" role="img" aria-label={label} title={label}>x</span>`,
      filename: 'src/lib/file-explorer/selection/TagDots.svelte',
      errors: [{ messageId: 'noTitleAttribute', data: { element: 'span' } }],
    },
    // One report per element, even in a nested tree.
    {
      code: `<div title="outer"><span title="inner">x</span></div>`,
      filename: 'src/lib/whatever/Row.svelte',
      errors: [
        { messageId: 'noTitleAttribute', data: { element: 'div' } },
        { messageId: 'noTitleAttribute', data: { element: 'span' } },
      ],
    },
  ],
})
