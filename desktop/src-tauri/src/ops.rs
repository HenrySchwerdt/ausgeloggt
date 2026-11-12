//! Small synchronous vault operations behind the account / newsletter commands.
//! Plain functions over `VaultData`, tested without Tauri.

use ausgeloggt_catalog::gdpr;
use ausgeloggt_core::domain::{canonical_service_domain, registrable_domain};
use ausgeloggt_core::merge;
use ausgeloggt_core::model::{
    AccountStatus, CategorySource, DeletionMethod, DeletionStep, Evidence, EvidenceKind, Finding,
    MailCategory, SourceKind, SubscriptionStatus, UnsubscribeAttempt, UnsubscribeMethod, VaultData,
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::{IpcError, IpcResult};

pub fn set_account_status(d: &mut VaultData, ids: &[Uuid], status: AccountStatus) -> IpcResult<()> {
    let now = Utc::now();
    let mut found = 0;
    for a in d.accounts.iter_mut().filter(|a| ids.contains(&a.id)) {
        a.status = status.clone();
        a.updated_at = now;
        found += 1;
    }
    if found == 0 && !ids.is_empty() {
        return Err(IpcError::not_found("Account"));
    }
    Ok(())
}

pub fn set_account_notes(d: &mut VaultData, id: Uuid, notes: String) -> IpcResult<()> {
    let a = d
        .accounts
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or_else(|| IpcError::not_found("Account"))?;
    a.notes = notes;
    a.updated_at = Utc::now();
    Ok(())
}

/// Manual account. Returns its id; an existing account with the same domain is reused.
pub fn add_account(
    d: &mut VaultData,
    domain: &str,
    name: Option<&str>,
    identifier: Option<&str>,
    now: DateTime<Utc>,
) -> IpcResult<Uuid> {
    let canonical = registrable_domain(domain)
        .map(|r| canonical_service_domain(&r))
        .ok_or_else(|| IpcError::invalid("Ungültige Domain"))?;
    let clean = |s: Option<&str>| s.map(str::trim).filter(|s| !s.is_empty()).map(String::from);
    let finding = Finding {
        domain: canonical.clone(),
        name: clean(name),
        identifier: clean(identifier),
        sso: None,
        evidence: Evidence {
            kind: EvidenceKind::Manual,
            source: SourceKind::Manual,
            summary: "Manuell hinzugefügt".into(),
            date: Some(now),
            source_ref: Some(format!("manual:{}", Uuid::new_v4())),
        },
    };
    merge::merge_findings(d, vec![finding]);
    let acct = d
        .accounts
        .iter_mut()
        .find(|a| a.domain == canonical)
        .ok_or_else(|| IpcError::invalid("Diese Domain kann nicht als Account angelegt werden"))?;
    if acct.status == AccountStatus::Discovered {
        acct.status = AccountStatus::Confirmed;
    }
    Ok(acct.id)
}

pub fn remove_accounts(d: &mut VaultData, ids: &[Uuid]) {
    d.accounts.retain(|a| !ids.contains(&a.id));
}

/// Record a deletion step. A GDPR e-mail starts the one month deadline. Status becomes
/// `DeletionPending`, except for `Confirmed` (the service confirmed): `Deleted`.
pub fn add_deletion_step(
    d: &mut VaultData,
    id: Uuid,
    method: DeletionMethod,
    note: String,
    now: DateTime<Utc>,
) -> IpcResult<()> {
    let a = d
        .accounts
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or_else(|| IpcError::not_found("Account"))?;
    let deadline =
        matches!(method, DeletionMethod::GdprEmail { .. }).then(|| gdpr::deadline_from(now));
    a.status = if matches!(method, DeletionMethod::Confirmed) {
        AccountStatus::Deleted
    } else {
        AccountStatus::DeletionPending
    };
    a.deletion.push(DeletionStep {
        at: now,
        method,
        note,
        deadline,
    });
    a.updated_at = now;
    Ok(())
}

/// Newsletter status chosen by the user. Marking a list `Unsubscribed` without an
/// automatic attempt (opened the link in the browser) records a manual attempt, so that
/// later mails can be recognised as "still receiving".
pub fn set_subscription_status(
    d: &mut VaultData,
    ids: &[Uuid],
    status: SubscriptionStatus,
    now: DateTime<Utc>,
) -> IpcResult<()> {
    let mut found = 0;
    for s in d.subscriptions.iter_mut().filter(|s| ids.contains(&s.id)) {
        found += 1;
        if status == SubscriptionStatus::Unsubscribed
            && !matches!(s.status, SubscriptionStatus::Unsubscribed)
        {
            let method = s
                .unsubscribe
                .first()
                .cloned()
                .unwrap_or(UnsubscribeMethod::Link { url: String::new() });
            s.history.push(UnsubscribeAttempt {
                at: now,
                method,
                ok: true,
                message: "Manuell abgemeldet".into(),
            });
        }
        s.status = status.clone();
    }
    if found == 0 && !ids.is_empty() {
        return Err(IpcError::not_found("Newsletter"));
    }
    Ok(())
}

pub fn set_subscription_category(
    d: &mut VaultData,
    ids: &[Uuid],
    category: MailCategory,
) -> IpcResult<()> {
    let mut found = 0;
    for s in d.subscriptions.iter_mut().filter(|s| ids.contains(&s.id)) {
        s.category = category;
        s.category_source = CategorySource::User;
        found += 1;
    }
    if found == 0 && !ids.is_empty() {
        return Err(IpcError::not_found("Newsletter"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ausgeloggt_core::model::Subscription;
    use chrono::Duration;

    fn d() -> VaultData {
        VaultData::default()
    }

    fn sub() -> Subscription {
        Subscription {
            id: Uuid::new_v4(),
            mailbox: "me@x.org".into(),
            list_key: "a@b.example".into(),
            domain: "b.example".into(),
            name: "B".into(),
            sender: "a@b.example".into(),
            unsubscribe: vec![UnsubscribeMethod::Link {
                url: "https://b.example/u".into(),
            }],
            message_count: 1,
            frequency_per_month: 0.0,
            first_seen: None,
            last_seen: None,
            sample_subjects: vec![],
            category: MailCategory::Marketing,
            category_source: CategorySource::Rules,
            status: SubscriptionStatus::Active,
            history: vec![],
            post_unsubscribe_mails: vec![],
            last_message_id: None,
        }
    }

    #[test]
    fn add_account_normalises_domain_and_confirms() {
        let mut data = d();
        let id = add_account(
            &mut data,
            "https://accounts.Spotify.com/login",
            Some(" Spotify "),
            Some("me@x.org"),
            Utc::now(),
        )
        .unwrap();
        let a = &data.accounts[0];
        assert_eq!(a.id, id);
        assert_eq!(a.domain, "spotify.com");
        assert_eq!(a.name, "Spotify");
        assert_eq!(a.identifiers, vec!["me@x.org"]);
        assert_eq!(a.status, AccountStatus::Confirmed);
        assert_eq!(a.evidence[0].kind, EvidenceKind::Manual);
        // adding the same service again reuses the account
        let again = add_account(&mut data, "spotify.com", None, None, Utc::now()).unwrap();
        assert_eq!(again, id);
        assert_eq!(data.accounts.len(), 1);
        assert!(add_account(&mut data, "kein domain", None, None, Utc::now()).is_err());
    }

    #[test]
    fn deletion_step_sets_status_and_gdpr_deadline() {
        let mut data = d();
        let id = add_account(&mut data, "spotify.com", None, None, Utc::now()).unwrap();
        let now = Utc::now();
        add_deletion_step(
            &mut data,
            id,
            DeletionMethod::GdprEmail {
                to: "p@spotify.com".into(),
            },
            "gesendet".into(),
            now,
        )
        .unwrap();
        let a = &data.accounts[0];
        assert_eq!(a.status, AccountStatus::DeletionPending);
        assert_eq!(a.deletion[0].deadline, Some(gdpr::deadline_from(now)));
        add_deletion_step(
            &mut data,
            id,
            DeletionMethod::DirectLink {
                url: "https://x".into(),
            },
            String::new(),
            now,
        )
        .unwrap();
        assert_eq!(data.accounts[0].deletion[1].deadline, None);
        add_deletion_step(&mut data, id, DeletionMethod::Confirmed, String::new(), now).unwrap();
        assert_eq!(data.accounts[0].status, AccountStatus::Deleted);
        assert!(add_deletion_step(
            &mut data,
            Uuid::new_v4(),
            DeletionMethod::Other,
            String::new(),
            now
        )
        .is_err());
    }

    #[test]
    fn status_and_notes_and_remove() {
        let mut data = d();
        let a = add_account(&mut data, "a.com", None, None, Utc::now()).unwrap();
        let b = add_account(&mut data, "b.com", None, None, Utc::now()).unwrap();
        set_account_status(&mut data, &[a, b], AccountStatus::Keep).unwrap();
        assert!(data
            .accounts
            .iter()
            .all(|x| x.status == AccountStatus::Keep));
        assert!(set_account_status(&mut data, &[Uuid::new_v4()], AccountStatus::Keep).is_err());
        set_account_notes(&mut data, a, "wichtig".into()).unwrap();
        assert_eq!(data.accounts[0].notes, "wichtig");
        remove_accounts(&mut data, &[a]);
        assert_eq!(data.accounts.len(), 1);
        assert_eq!(data.accounts[0].id, b);
    }

    #[test]
    fn manual_unsubscribe_records_an_attempt_once() {
        let mut data = d();
        let s = sub();
        let id = s.id;
        data.subscriptions.push(s);
        let now = Utc::now();
        set_subscription_status(&mut data, &[id], SubscriptionStatus::Unsubscribed, now).unwrap();
        assert_eq!(data.subscriptions[0].history.len(), 1);
        assert!(data.subscriptions[0].history[0].ok);
        // setting it again does not duplicate
        set_subscription_status(
            &mut data,
            &[id],
            SubscriptionStatus::Unsubscribed,
            now + Duration::days(1),
        )
        .unwrap();
        assert_eq!(data.subscriptions[0].history.len(), 1);
        set_subscription_status(&mut data, &[id], SubscriptionStatus::Keep, now).unwrap();
        assert_eq!(data.subscriptions[0].status, SubscriptionStatus::Keep);
        assert!(set_subscription_status(
            &mut data,
            &[Uuid::new_v4()],
            SubscriptionStatus::Keep,
            now
        )
        .is_err());
    }

    #[test]
    fn category_by_user_is_sticky() {
        let mut data = d();
        let s = sub();
        let id = s.id;
        data.subscriptions.push(s);
        set_subscription_category(&mut data, &[id], MailCategory::Newsletter).unwrap();
        assert_eq!(data.subscriptions[0].category, MailCategory::Newsletter);
        assert_eq!(data.subscriptions[0].category_source, CategorySource::User);
    }
}
