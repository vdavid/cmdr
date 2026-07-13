//! The consent gate: Ask Cmdr's privacy line made STRUCTURAL, not just a UI affordance.
//!
//! The rail's frontend gate is the UX layer; this is the backend enforcement. Every send
//! (`commands::agent::ask_cmdr_send_message`) checks [`has_current_consent`] before it
//! creates a thread or resolves the LLM, so nothing reaches a provider without a recorded,
//! current opt-in — the claim `agent/CLAUDE.md` and the consent screen make.

use rusqlite::Connection;

use crate::agent::store;

/// The consent-copy version the user must have accepted for the gate to open. **Bump this
/// whenever the `askCmdr.consent.*` copy changes materially**, so a stale acceptance no
/// longer counts and users re-consent to the new wording. The copy itself lives in the
/// frontend catalog; this integer is its machine-checkable version, recorded in `main.db`.
pub const CONSENT_COPY_VERSION: u32 = 1;

/// Whether the user has accepted the CURRENT consent copy. Fails CLOSED: an absent record,
/// a stale version, or an unreadable store all read as "not consented", so a send is
/// refused rather than proceeding on doubt.
pub fn has_current_consent(conn: &Connection) -> bool {
    matches!(store::get_consent(conn), Ok(Some(consent)) if consent.version == CONSENT_COPY_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn migrated_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory db");
        store::run_migrations(&conn, store::MIGRATIONS).expect("migrate");
        conn
    }

    #[test]
    fn no_record_is_not_consented() {
        let conn = migrated_conn();
        assert!(
            !has_current_consent(&conn),
            "a fresh DB with no consent record ⇒ gate closed"
        );
    }

    #[test]
    fn a_stale_copy_version_is_not_consented() {
        let conn = migrated_conn();
        // An older accepted version no longer counts once the copy (and the constant) moved on.
        store::set_consent(&conn, CONSENT_COPY_VERSION.wrapping_sub(1), 1_780_000_000).expect("set");
        assert!(!has_current_consent(&conn), "a stale copy version ⇒ gate closed");
    }

    #[test]
    fn the_current_copy_version_is_consented() {
        let conn = migrated_conn();
        store::set_consent(&conn, CONSENT_COPY_VERSION, 1_780_000_000).expect("set");
        assert!(has_current_consent(&conn), "accepting the current copy ⇒ gate open");
    }
}
