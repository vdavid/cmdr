# Plan: centralized ModalDialog component

## Why

- 10 dialog components each duplicate ~40 lines of overlay/container/title/focus/keyboard CSS and logic
- Only 4 of 10 dialogs send MCP notifications (CopyDialog, NewFolderDialog, About, License — and the last two are wired manually in `+page.svelte`). The rest are invisible to the MCP server.
- DraggableDialog already exists as a partial extraction — evolve it into the full solution.

## Why not Ark UI Dialog?

Ark UI is already a dependency (ADR 017), but its Dialog component doesn't save us much here:
- Focus trapping: only 1 dialog needs it (CopyProgressDialog). Not worth the full Dialog machinery.
- Scroll lock / portal: irrelevant for a fixed-viewport file manager.
- MCP notifications, dragging, two backdrop variants, custom escape handling: all still need custom code on top of Ark.
- Migrating 10 dialogs to Ark's compositional API (Root/Backdrop/Positioner/Content/Title) is high effort for marginal gain.

A thin custom ModalDialog directly solves the real problems (duplication, MCP discoverability).

## ModalDialog design

**File:** `apps/desktop/src/lib/ui/ModalDialog.svelte` (renamed from DraggableDialog)

```typescript
interface Props {
    titleId: string
    onkeydown: (event: KeyboardEvent) => void
    title: Snippet
    children: Snippet
    dialogId?: string             // MCP: notifyDialogOpened/Closed on mount/destroy
    role?: 'dialog' | 'alertdialog'  // default 'dialog'
    draggable?: boolean           // default false
    blur?: boolean                // default false (false=0.4 opacity, true=0.6+blur)
    ariaDescribedby?: string      // optional
    containerStyle?: string       // inline styles for sizing/colors
}
```

**Sizing/colors via `containerStyle` prop** (inline styles, because stylelint blocks non-`(color|spacing|font)-` prefixed CSS custom properties):
- Each dialog sets its own width/min-width/max-width
- Dialogs needing different bg/border colors override via inline `background`/`border-color`

## 10 dialogs migrated

| # | Dialog | blur | role | draggable | dialogId | containerStyle |
|---|--------|------|------|-----------|----------|----------------|
| 1 | CopyDialog | - | dialog | yes | copy-confirmation | min-width: 420px; max-width: 500px |
| 2 | CopyProgressDialog | - | dialog | yes | copy-progress | min-width: 420px; max-width: 500px |
| 3 | AlertDialog | - | alertdialog | - | alert | width: 360px |
| 4 | ExpirationModal | yes | dialog | - | expiration | max-width: 420px; bg/border overrides |
| 5 | CommercialReminderModal | yes | dialog | - | commercial-reminder | max-width: 420px; bg/border overrides |
| 6 | CopyErrorDialog | - | alertdialog | - | copy-error | width: 420px; max-width: 90vw; bg/border overrides |
| 7 | NewFolderDialog | - | dialog | - | mkdir-confirmation | width: 400px |
| 8 | LicenseKeyDialog | yes | dialog | - | license | min-width: 400px; max-width: 500px |
| 9 | AboutWindow | yes | dialog | - | about | min-width: 380px; max-width: 480px |
| 10 | PtpcameradDialog | yes | dialog | - | ptpcamerad | min-width: 480px; max-width: 560px |

**Not in scope:** CommandPalette (no title bar, search-based UI, too different), NetworkLoginForm (inline, not a modal).

## Milestones (all completed)

### M0: Create ModalDialog

- [x] Rename DraggableDialog -> ModalDialog
- [x] Add props: `role`, `blur`, `dialogId`, `ariaDescribedby`, `draggable` (default false), `containerStyle`
- [x] Add MCP notification in onMount/onDestroy (guarded by `dialogId`)
- [x] Add `.blur` overlay variant (0.6 + backdrop-filter)
- [x] Update `coverage-allowlist.json`

### M1: Migrate existing consumers (CopyDialog, CopyProgressDialog)

- [x] Update imports, add `draggable` prop
- [x] CopyDialog: move MCP notification to `dialogId` prop, remove manual calls
- [x] CopyProgressDialog: add `dialogId="copy-progress"` (new MCP tracking)

### M2: Migrate simple dialogs (AlertDialog, ExpirationModal, CommercialReminderModal)

- [x] Per dialog: replace overlay+container+h2 markup with `<ModalDialog>`, remove duplicated CSS, add `dialogId`
- [x] ExpirationModal + CommercialReminderModal override container colors via `containerStyle`

### M3: Migrate medium dialogs (CopyErrorDialog, NewFolderDialog)

- [x] CopyErrorDialog: `role="alertdialog"`, override colors via `containerStyle`
- [x] NewFolderDialog: remove manual MCP notification calls (centralized in ModalDialog)

### M4: Migrate complex dialogs (LicenseKeyDialog, AboutWindow, PtpcameradDialog)

- [x] Close buttons (x) kept in consumers, positioned absolute within container
- [x] AboutWindow: changed h1 -> h2 (semantic fix for modal)
- [x] Removed manual MCP notification calls from `+page.svelte` for about/license

### M5: Clean up

- [x] Audited `+page.svelte` to remove all redundant `notifyDialogOpened`/`notifyDialogClosed` calls
- [x] Removed unused `--color-error-text` CSS variable from `app.css`
- [x] All 11 Svelte checks pass: `./scripts/check.sh --svelte`

## Impact

- ~175 lines of duplicated CSS/JS removed
- MCP dialog coverage: 4/10 -> 10/10
- Every new dialog gets overlay, focus, keyboard, MCP tracking for free
