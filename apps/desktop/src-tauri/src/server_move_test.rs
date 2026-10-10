//! The password's half of a move, on its own: what a store that refuses, or has
//! nothing, does to it. The whole move, through the command the sheet calls:
//! `commands/server_moves_test.rs`.

use super::*;

fn stored() -> Result<SmbCredentials, KeychainError> {
    Ok(SmbCredentials {
        username: "ada".to_string(),
        password: "pa55".to_string(),
    })
}

#[test]
fn a_password_the_store_wont_take_at_the_new_address_refuses_the_move() {
    let copied = copy_secret_with(stored, |_| Err(KeychainError::AccessDenied("Deny".to_string())));

    assert!(
        copied.is_err(),
        "a server whose password stayed behind would ask for it again"
    );
}

#[test]
fn a_password_the_store_wont_read_refuses_the_move() {
    let copied = copy_secret_with(|| Err(KeychainError::Other("locked".to_string())), |_| Ok(()));

    assert!(copied.is_err());
}

#[test]
fn nothing_stored_is_nothing_to_move() {
    let copied = copy_secret_with(
        || Err(KeychainError::NotFound("none".to_string())),
        |_| panic!("nothing to write"),
    );

    assert!(matches!(copied, Ok(SecretCopy::NothingStored)));
}

#[test]
fn a_stored_password_is_written_under_the_new_address() {
    let mut written = None;
    let copied = copy_secret_with(stored, |creds| {
        written = Some(creds.password.clone());
        Ok(())
    });

    assert!(matches!(copied, Ok(SecretCopy::Copied)));
    assert_eq!(written.as_deref(), Some("pa55"));
}
