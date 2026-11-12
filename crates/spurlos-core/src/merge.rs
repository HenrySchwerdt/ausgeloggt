//! Merge findings into accounts.

use crate::model::{Evidence, Finding, VaultData};

/// Prefix for findings whose domain is not known, only a service name.
pub const UNRESOLVED_PREFIX: &str = "unresolved:";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    pub findings: u64,
    pub new_accounts: u64,
    pub updated_accounts: u64,
    /// Findings dropped (unparsable domain, infrastructure domain, duplicate evidence).
    pub skipped: u64,
}

/// Fold findings into `data.accounts`:
/// - group by `canonical_service_domain(registrable_domain(f.domain))`; drop
///   unparsable and infrastructure domains (count as skipped)
/// - special case: `f.domain` starting with [`UNRESOLVED_PREFIX`] (e.g. an SSO grant
///   mail only names the app, `unresolved:notion`) is kept verbatim as the key; if an
///   existing account's name matches `f.name` case-insensitively, attach to that instead
/// - dedupe evidence by (source, source_ref); without source_ref by (source, kind, summary, date).
///   Re-importing the same file must be a no-op.
/// - union identifiers (case-insensitive) and sso providers
/// - name: prefer an explicit finding name over `display_name_for`, never overwrite a
///   name on an existing account
/// - update first_seen / last_seen from evidence dates
/// - recompute confidence via [`confidence`]
/// - never change a status the user has set (anything but `Discovered`)
/// - set `updated_at` only on accounts that actually changed
pub fn merge_findings(data: &mut VaultData, findings: Vec<Finding>) -> MergeReport {
    let _ = (data, findings);
    todo!("implemented by core agent")
}

/// Fold scan observations into `data.subscriptions`, keyed by (mailbox, list_key):
/// - message_count adds up only for new mails (the scan is incremental; observations
///   carry counts for *new* mails only)
/// - first/last_seen widen; sample_subjects: newest first, dedup, max 5
/// - unsubscribe methods: replaced by the newest observation's if non-empty
/// - category: rules may update only if `category_source == Rules`
/// - frequency_per_month recomputed from count and first/last_seen
/// - status: if `Unsubscribed` and a mail with date > last unsubscribe attempt + 7 days
///   arrives -> `StillReceiving`
/// Returns (new, updated) counts.
pub fn merge_subscriptions(
    data: &mut VaultData,
    observations: Vec<crate::model::SubscriptionObservation>,
) -> (u64, u64) {
    let _ = (data, observations);
    todo!("implemented by core agent")
}

/// Confidence from evidence: `1 - Π(1 - weight)` over the *distinct* evidence kinds.
pub fn confidence(evidence: &[Evidence]) -> f32 {
    let _ = evidence;
    todo!("implemented by core agent")
}
