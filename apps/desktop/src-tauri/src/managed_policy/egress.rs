//! Every pipeline that sends bytes to `api.getcmdr.com` (or another Cmdr-chosen server), and the
//! one table that says which policy key stops it.

use super::{ManagedPolicy, UpdatePolicy};

/// A pipeline that sends data off the Mac. Every sender names one, so adding a pipeline forces a
/// decision in [`ManagedPolicy::allows`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Egress {
    Heartbeat,
    CrashReport,
    ErrorReport,
    ErrorReportAmend,
    // Only the macOS updater sends these, but they stay on every platform: `allows` matches
    // exhaustively, and the variant list is the code form of `/trust`'s egress table.
    #[cfg_attr(
        not(any(target_os = "macos", test)),
        expect(dead_code, reason = "only the macOS updater sends an update check")
    )]
    UpdateCheck,
    #[cfg_attr(
        not(any(target_os = "macos", test)),
        expect(dead_code, reason = "only the macOS updater downloads an update")
    )]
    UpdateDownload,
    S3PriceList,
}

impl ManagedPolicy {
    /// Whether `egress` may send right now. The always-`true` arms are the code form of the
    /// "traffic no key turns off" list on `/trust`. The update ceiling isn't judged here: it needs
    /// the version, which the macOS updater checks with
    /// [`UpdateCeiling`](super::UpdateCeiling)`::allows`.
    pub fn allows(&self, egress: Egress) -> bool {
        match egress {
            Egress::Heartbeat => !self.usage_stats_disabled(),
            Egress::CrashReport | Egress::ErrorReport | Egress::ErrorReportAmend => !self.reports_disabled(),
            Egress::UpdateCheck | Egress::UpdateDownload => self.updates() != UpdatePolicy::Disabled,
            Egress::S3PriceList => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_policy::UpdateCeiling;

    const ALL: [Egress; 7] = [
        Egress::Heartbeat,
        Egress::CrashReport,
        Egress::ErrorReport,
        Egress::ErrorReportAmend,
        Egress::UpdateCheck,
        Egress::UpdateDownload,
        Egress::S3PriceList,
    ];

    fn blocked(policy: &ManagedPolicy) -> Vec<Egress> {
        ALL.into_iter().filter(|e| !policy.allows(*e)).collect()
    }

    #[test]
    fn no_policy_blocks_nothing() {
        assert_eq!(blocked(&ManagedPolicy::default()), vec![]);
    }

    #[test]
    fn usage_stats_off_blocks_only_the_heartbeat() {
        let policy = ManagedPolicy {
            usage_stats_disabled: true,
            ..Default::default()
        };
        assert_eq!(blocked(&policy), vec![Egress::Heartbeat]);
    }

    #[test]
    fn reports_off_blocks_crash_error_and_amend() {
        let policy = ManagedPolicy {
            reports_disabled: true,
            ..Default::default()
        };
        assert_eq!(
            blocked(&policy),
            vec![Egress::CrashReport, Egress::ErrorReport, Egress::ErrorReportAmend]
        );
    }

    #[test]
    fn updates_off_blocks_check_and_download() {
        let policy = ManagedPolicy {
            updates_disabled: true,
            ..Default::default()
        };
        assert_eq!(blocked(&policy), vec![Egress::UpdateCheck, Egress::UpdateDownload]);
    }

    #[test]
    fn no_automatic_checks_and_a_ceiling_leave_manual_checks_alone() {
        let policy = ManagedPolicy {
            automatic_update_checks_disabled: true,
            update_ceiling: Some(UpdateCeiling::Minor(0, 52)),
            ..Default::default()
        };
        assert_eq!(blocked(&policy), vec![]);
    }

    #[test]
    fn ai_keys_block_no_api_server_traffic() {
        let policy = ManagedPolicy {
            ai_disabled: true,
            cloud_ai_disabled: true,
            allowed_cloud_ai_hosts: Some(vec![]),
            ..Default::default()
        };
        assert_eq!(blocked(&policy), vec![]);
    }

    #[test]
    fn every_key_at_once_still_lets_the_s3_price_list_through() {
        let policy = ManagedPolicy {
            usage_stats_disabled: true,
            reports_disabled: true,
            automatic_update_checks_disabled: true,
            updates_disabled: true,
            update_ceiling: Some(UpdateCeiling::Major(0)),
            ai_disabled: true,
            cloud_ai_disabled: true,
            allowed_cloud_ai_hosts: Some(vec![]),
        };
        assert!(policy.allows(Egress::S3PriceList));
        assert_eq!(blocked(&policy).len(), ALL.len() - 1);
    }
}
