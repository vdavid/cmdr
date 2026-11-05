//! "Always require SMB encryption": the person's setting (`network.requireSmbEncryption`) OR
//! the organization's `RequireSMBEncryption`, pushed into `cmdr_smb::SmbEncryption`, which every
//! SMB session Cmdr builds reads (`crates/cmdr-smb/DETAILS.md` § "Requiring encryption").
//!
//! Three inputs, one decision ([`effective`]): the setting read off disk at launch ([`init`],
//! before any volume is built), the settings applier's live pushes ([`set_user_choice`]), and
//! every policy change ([`on_policy_applied`]). ❗ A flip reaches the next session built, never
//! one that's already up.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::managed_policy::ManagedPolicy;

static USER_CHOICE: AtomicBool = AtomicBool::new(false);

/// Whether sessions must be encrypted: the person asked, or the organization requires it.
pub(crate) fn effective(user_choice: bool, policy: &ManagedPolicy) -> bool {
    user_choice || policy.smb_encryption_required()
}

/// At launch, from the setting as stored (absent reads as off, its default), ❗ before any SMB
/// volume is built: a launch-time upgrade would otherwise open its sessions in the clear.
pub fn init(stored_choice: Option<bool>) {
    USER_CHOICE.store(stored_choice.unwrap_or(false), Ordering::Release);
    apply(&crate::managed_policy::current());
}

/// The person's switch, pushed live by the settings applier.
pub fn set_user_choice(required: bool) {
    USER_CHOICE.store(required, Ordering::Release);
    apply(&crate::managed_policy::current());
}

/// The organization's policy changed (`managed_policy::apply_change`).
pub fn on_policy_applied(new: &ManagedPolicy) {
    apply(new);
}

fn apply(policy: &ManagedPolicy) {
    let required = effective(USER_CHOICE.load(Ordering::Acquire), policy);
    if required != cmdr_smb::SmbEncryption::is_required() {
        log::info!(
            target: "network::smb_encryption",
            "SMB encryption is {} for sessions built from now on",
            if required { "required" } else { "no longer required" }
        );
    }
    cmdr_smb::SmbEncryption::set_required(required);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_policy::testing::{REQUIRE_SMB_ENCRYPTION, forcing};

    #[test]
    fn either_the_person_or_the_organization_requires_it() {
        let none = ManagedPolicy::default();
        let required = forcing(&[REQUIRE_SMB_ENCRYPTION]);
        assert!(!effective(false, &none));
        assert!(effective(true, &none));
        assert!(effective(false, &required), "the policy forces it on");
        assert!(effective(true, &required));
    }

    /// The switch the sessions read follows both inputs live. Nextest runs each test in its own
    /// process, so the global flips here reach no other test.
    #[test]
    fn the_session_switch_follows_the_setting_and_the_policy() {
        init(None);
        assert!(!cmdr_smb::SmbEncryption::is_required(), "off by default");
        set_user_choice(true);
        assert!(cmdr_smb::SmbEncryption::is_required());
        set_user_choice(false);
        assert!(!cmdr_smb::SmbEncryption::is_required());
        on_policy_applied(&forcing(&[REQUIRE_SMB_ENCRYPTION]));
        assert!(cmdr_smb::SmbEncryption::is_required(), "the policy alone requires it");
        on_policy_applied(&ManagedPolicy::default());
        assert!(!cmdr_smb::SmbEncryption::is_required());
        cmdr_smb::SmbEncryption::set_required(false);
    }
}
