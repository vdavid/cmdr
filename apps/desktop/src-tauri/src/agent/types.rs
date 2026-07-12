//! The typed vocabulary of the agent store: token-backed enums for the classification
//! columns of `main.db`, and the `token_enum!` macro that declares them.
//!
//! Every classification field the store persists or crosses a boundary with is a typed
//! enum here, never a substring branch (`no-string-matching`). Each carries a compact,
//! stable, human-readable snake_case **token** stored as TEXT, so the DB stays
//! `sqlite3`-inspectable and the enum ↔ storage mapping lives in exactly one place.
//! Renaming a token is a schema change; renaming a variant is free.
//!
//! `AgentRole` (the `messages.role` column) and `ProviderTag` (the `cost_meter.provider`
//! column) are token-backed too, but they live in [`super::llm::types`] because the LLM
//! seam owns them; this module carries the store-only enums.
//!
//! The `token_enum!` macro mirrors the operation log's (`operation_log/types.rs`): the
//! two are deliberately separate copies of a tiny code-generator so each durable store
//! stays self-contained, with no cross-subsystem macro coupling.

/// Declare a token-backed enum once: the variants, their stable DB tokens, `as_token`,
/// and `from_token`. Keeps the two directions in lockstep so they can't drift. The
/// serde/specta wire form (camelCase, for IPC + `bindings.ts`) is SEPARATE from the DB
/// `as_token` (stable snake_case): callers cross IPC as this typed enum, never a string
/// (`no-string-matching`); the store reads/writes via the tokens.
macro_rules! token_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident { $( $(#[$vmeta:meta])* $variant:ident => $token:literal ),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, specta::Type)]
        #[serde(rename_all = "camelCase")]
        $vis enum $name { $( $(#[$vmeta])* $variant ),+ }

        impl $name {
            /// The stable DB token for this variant.
            pub fn as_token(self) -> &'static str {
                match self { $( $name::$variant => $token ),+ }
            }

            /// Parse a stored token back to the variant, or `None` if unknown.
            pub fn from_token(token: &str) -> Option<Self> {
                match token { $( $token => Some($name::$variant), )+ _ => None }
            }
        }
    };
}

token_enum! {
    /// How a conversation was started, stored in the nullable `conversations.origin`
    /// column. NULL means the user started it (the v1 case); a non-null token records a
    /// programmatic origin. Kept as a column (not a migration) so a future
    /// notification-spawned thread is an additive token, not a schema change (spec §3).
    /// v1 never writes a non-null origin; `Notification` is the forward-compat surface
    /// the column exists to hold.
    pub enum ConversationOrigin {
        Notification => "notification",
    }
}
