# Tool questions: details

The backend half, and why the whole thing is shaped this way: `apps/desktop/src-tauri/src/tools/DETAILS.md` § The human
gate.

## Flow

1. `tool-ask-requested` reaches `receiveToolAsk`. A modal that opened since the backend's read answers `blocked`, naming
   itself, and the question never goes up.
2. `ToolAskHost` renders `ToolAskDialog` under the arming guard, which registers `tool-ask` in the tracker like any
   `ModalDialog`, so `cmdr://state` shows it.
3. A person's answer goes through `answer_tool_ask`. The backend's wait ends and sends `tool-ask-dismissed`, which is a
   no-op by then. A Stop, a client gone, or the expiry sends the same event with the question still up, which takes it
   down.

## Decisions

- **Decision: a yes isn't the end of the check.** The dialog says yes and forgets. The backend re-reads what it asked
  about, so this module holds no copy of what the question promised.
- **Decision: the popup is skipped after a yes.** `open_under_cursor` on an entry set to Ask would otherwise raise the
  Browse | Open popup right after the question, with the first row a stray Enter away. The person already said to open
  it, so `entry-activation.ts::openForTool` opens it. A caller off direct mode with no yes can't launch at all, so it
  never reaches the popup.
- **Decision: a setting question shows its values as rows, friendly name plus raw id.** A sentence like "from system to
  com.apple.TextEdit" hid what changes. The backend decides the name (`SettingValueFact` in `tools/ask.rs`, built by
  `value_fact` in `tools/ask_plans.rs`): the registry's option label, else an app-picker value's app name, else none.
  The text editor's `system` comes as a typed `systemDefault` so the frontend words it per language. A name that only
  repeats the raw value is dropped; an empty value reads "nothing" with no code chip. The raw id stays visible because
  it's what actually gets written.
- **Gotcha: a question sent while no listener is up waits out the expiry**: the window between a reload's start and
  `setupToolAsk`. `register_known_dialogs` wakes one sent before the reload, and the expiry bounds the rest.
