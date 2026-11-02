//! What a call's tool line names. A leaf, so `ToolId` can answer it without reaching the handlers.

/// What a call's tool line names, read off its arguments by `agent/line/`. The strings are argument
/// keys. A tool here flows through the line's one generic arm: its subject comes from this, its
/// outcome reads as an answer unless its result carries a typed refusal, and its preview has no rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectSpec {
    /// Nothing to name: the tool's own words say it all.
    None,
    /// A place, at this argument.
    Path(&'static str),
    /// A volume id, at this argument.
    Volume(&'static str),
    /// An operation id, at this argument.
    Operation(&'static str),
    /// The pane the `pane` argument names; an absent one means the focused pane.
    Pane,
    /// The setting the `id` argument names.
    Setting,
    /// `read_resource`: which `cmdr://` view its `uri` names.
    Resource,
    /// A rich tool: `agent/line/` has hand arms for its subject, outcome, and preview.
    Custom,
}
