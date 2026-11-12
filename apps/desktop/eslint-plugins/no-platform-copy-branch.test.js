import { RuleTester } from 'eslint'
import * as svelteParser from 'svelte-eslint-parser'
import rule from './no-platform-copy-branch.js'

// Flat-config RuleTester (ESLint 9+). RuleTester auto-detects Vitest's `describe`/`it`
// globals and emits one test per case, so `run` sits at the top level.
const tsTester = new RuleTester({ languageOptions: { ecmaVersion: 'latest', sourceType: 'module' } })
const svelteTester = new RuleTester({
  languageOptions: { parser: svelteParser, ecmaVersion: 'latest', sourceType: 'module' },
})

const ts = 'src/lib/commands/sources/file-list.ts'
const svelte = 'src/lib/mtp/MtpConnectedToastContent.svelte'

tsTester.run('cmdr/no-platform-copy-branch', rule, {
  valid: [
    // The intended path: one key whose value is a platform block; the runtime picks the branch.
    { code: `const nameKey = 'commands.fileShowInFileManager.label'`, filename: ts },
    { code: `const label = tString('commands.fileShowInFileManager.label')`, filename: ts },
    // Showing or hiding a surface is a capability question, and its arms may hold copy.
    {
      code: `const label = hasCapability('quick_look') ? tString('commands.fileQuickLook.label') : undefined`,
      filename: ts,
    },
    { code: `if (hasCapability('get_info')) items.push(t('commands.fileGetInfo.label'))`, filename: ts },
    // Key-glyph formatting is not copy: a shortcut reads in its OS's symbols.
    {
      code: `const hint = getEffectiveShortcuts('file.deletePermanently')[0] ?? (isMacOS() ? '⇧F8' : 'Shift+F8')`,
      filename: 'src/lib/file-operations/transfer/transfer-error-messages.ts',
    },
    // A platform test that guards behavior, with no copy in its arms.
    { code: `function f() { if (!isMacOS()) return; startWatcher() }`, filename: ts },
    { code: `const modifier = getPlatform() === 'macos' ? 'metaKey' : 'ctrlKey'`, filename: ts },
    // Copy outside the conditional is fine; the platform test only picks a non-copy value.
    { code: `const sep = isMacOS() ? '⌘' : 'Ctrl+'; const text = tString('a.b', { sep })`, filename: ts },
  ],
  invalid: [
    // The `file-list.ts` shape: a ternary choosing between two message keys.
    {
      code: `const entry = { nameKey: isMacOS() ? 'commands.fileShowInFinder.mac.label' : 'commands.fileShowInFinder.other.label' }`,
      filename: ts,
      errors: [{ messageId: 'platformCopyBranch' }],
    },
    // The `ai-secret-error.ts` shapes: a nested ternary, and an `if` returning copy.
    {
      code: `const body = denied ? (isMacOS() ? tString('ai.secretError.removeKeychainBody') : tString('ai.secretError.keyringBody')) : tString('ai.secretError.genericBody')`,
      filename: 'src/lib/settings/sections/ai-secret-error.ts',
      errors: [{ messageId: 'platformCopyBranch' }],
    },
    {
      code: `function describe(op) {
        if (isMacOS()) {
          return { title: tString('ai.secretError.keychainTitle', { op }) }
        }
        return { title: tString('ai.secretError.keyringTitle', { op }) }
      }`,
      filename: 'src/lib/settings/sections/ai-secret-error.ts',
      errors: [{ messageId: 'platformCopyBranch' }],
    },
    // `getPlatform()` compared, and the raw family's `getMessage`.
    {
      code: `const s = getPlatform() === 'windows' ? getMessage('errors.x.windows') : getMessage('errors.x')`,
      filename: ts,
      errors: [{ messageId: 'platformCopyBranch' }],
    },
    // A platform test saved to a name first (the transfer helper's `const mac = isMacOS()`).
    {
      code: `function pick() { const mac = isMacOS(); return mac ? w('fileLocked.suggestion.mac') : w('fileLocked.suggestion.other') }`,
      filename: 'src/lib/file-operations/transfer/transfer-error-messages.ts',
      errors: [{ messageId: 'platformCopyBranch' }],
    },
    // The startup nudge: a ternary of two `tString` calls.
    {
      code: `const message = isMacOS() ? tString('main.upgradeNudge.mac') : tString('main.upgradeNudge.other')`,
      filename: 'src/routes/(main)/startup-gates.ts',
      errors: [{ messageId: 'platformCopyBranch' }],
    },
  ],
})

svelteTester.run('cmdr/no-platform-copy-branch (Svelte)', rule, {
  valid: [
    { code: `<p>{tString('mtp.connectedToast.body')}</p>`, filename: svelte },
    // A capability gate in markup, with copy inside.
    { code: `{#if hasCapability('quick_look')}<p>{tString('a.b')}</p>{/if}`, filename: svelte },
    // A platform test in markup that holds no copy (an OS-shaped glyph).
    { code: `{#if isMacOS()}<Icon name="command" />{:else}<span>Ctrl</span>{/if}`, filename: svelte },
  ],
  invalid: [
    // The `MtpConnectedToastContent.svelte` shape.
    {
      code: `<script>import { isMacOS } from '#lib/platform/index.js'</script>
<p>{#if isMacOS()}{tString('mtp.connectedToast.bodyMac')}{:else}{tString('mtp.connectedToast.bodyOther')}{/if}</p>`,
      filename: svelte,
      errors: [{ messageId: 'platformCopyBranch' }],
    },
    // The `AppearanceSection.svelte` shape: a ternary inside a mustache.
    {
      code: `<LinkButton>{isMacOS() ? tString('settings.appearance.appColorHintLinkMac') : tString('settings.appearance.appColorHintLinkOther')}</LinkButton>`,
      filename: 'src/lib/settings/sections/AppearanceSection.svelte',
      errors: [{ messageId: 'platformCopyBranch' }],
    },
    // `<Trans>` in an arm resolves copy too.
    {
      code: `{#if isMacOS()}<Trans key="a.mac" />{:else}<Trans key="a.other" />{/if}`,
      filename: svelte,
      errors: [{ messageId: 'platformCopyBranch' }],
    },
  ],
})
