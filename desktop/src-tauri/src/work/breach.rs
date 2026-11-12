//! Breach check job (public HIBP catalogue; optional per-address lookups with a key)
//! and the catalog refresh job.

use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use ausgeloggt_breach::{BreachError, CatalogBreach};
use ausgeloggt_catalog::Catalog;
use ausgeloggt_core::merge;
use ausgeloggt_core::model::{AccountStatus, BreachRecord, VaultData};
use serde_json::json;

use crate::error::{IpcError, IpcResult};
use crate::jobs::{JobCtx, JobFail, JobOutcome, JobResult};
use crate::state::AppState;

type CatalogueFn = Box<dyn Fn() -> Result<Vec<CatalogBreach>, BreachError> + Send + Sync>;
type AddressFn = Box<dyn Fn(&str, &str) -> Result<Vec<BreachRecord>, BreachError> + Send + Sync>;

pub struct BreachApi {
    pub catalogue: CatalogueFn,
    pub for_address: AddressFn,
    /// Pause between two per-address lookups (HIBP rate limit).
    pub pause: Duration,
}

impl BreachApi {
    pub fn real() -> Self {
        Self {
            catalogue: Box::new(ausgeloggt_breach::fetch_breach_catalogue),
            for_address: Box::new(ausgeloggt_breach::breaches_for_address),
            pause: Duration::from_millis(1600),
        }
    }
}

pub fn start_breach_check(state: &AppState, api: BreachApi) -> IpcResult<String> {
    let st = state.clone();
    state.start_job("breach_check", move |ctx| breach_job(ctx, &st, &api))
}

fn breach_count(d: &VaultData) -> usize {
    d.accounts.iter().map(|a| a.security.breaches.len()).sum()
}

fn confirmed_count(d: &VaultData) -> usize {
    d.accounts
        .iter()
        .flat_map(|a| a.security.breaches.iter())
        .filter(|b| b.confirmed_for_user)
        .count()
}

/// Distinct e-mail identifiers to look up: mailbox addresses and identifiers of
/// accounts the user has not dismissed.
fn addresses_to_check(d: &VaultData) -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    for m in &d.mailboxes {
        let a = m.address.trim().to_lowercase();
        if a.contains('@') {
            set.insert(a);
        }
    }
    for acc in d.accounts.iter().filter(|a| {
        !matches!(
            a.status,
            AccountStatus::NotAnAccount | AccountStatus::Ignored
        )
    }) {
        for i in &acc.identifiers {
            let i = i.trim().to_lowercase();
            if i.contains('@') && !i.contains(char::is_whitespace) {
                set.insert(i);
            }
        }
    }
    set.into_iter().collect()
}

/// Sleep in small steps so cancelling stays responsive.
fn pause(ctx: &JobCtx, total: Duration) -> Result<(), JobFail> {
    let step = Duration::from_millis(50);
    let mut waited = Duration::ZERO;
    while waited < total {
        ctx.check()?;
        let s = step.min(total - waited);
        std::thread::sleep(s);
        waited += s;
    }
    Ok(())
}

pub fn breach_job(ctx: &JobCtx, state: &AppState, api: &BreachApi) -> JobResult {
    ctx.progress(0, 1, "Lade die öffentliche Datenleck Liste");
    let catalogue = (api.catalogue)().map_err(IpcError::from)?;
    ctx.check()?;
    let before = state.read(breach_count)?;
    let domain_hits: Vec<(String, BreachRecord)> = catalogue
        .iter()
        .filter(|b| !b.domain.trim().is_empty())
        .map(|b| (b.domain.clone(), b.record.clone()))
        .collect();
    state.mutate(|d| merge::merge_breaches(d, domain_hits))?;

    // Optional per-address lookups (need the user's own HIBP key).
    let (key, addresses) = state.read(|d| {
        (
            d.settings.hibp_api_key.trim().to_string(),
            addresses_to_check(d),
        )
    })?;
    let mut lookups = 0u64;
    let mut confirmed_new = 0u64;
    let mut lookup_error: Option<IpcError> = None;
    if !key.is_empty() && !addresses.is_empty() {
        // HIBP names -> domain, to attach address hits to the right accounts.
        let by_name: HashMap<&str, &str> = catalogue
            .iter()
            .filter(|b| !b.domain.trim().is_empty())
            .map(|b| (b.record.name.as_str(), b.domain.as_str()))
            .collect();
        let total = addresses.len() as u64 + 1;
        let confirmed_before = state.read(confirmed_count)?;
        for (i, addr) in addresses.iter().enumerate() {
            ctx.check()?;
            ctx.progress(
                i as u64 + 1,
                total,
                format!("Prüfe Adresse {} von {}", i + 1, addresses.len()),
            );
            match (api.for_address)(&key, addr) {
                Ok(records) => {
                    lookups += 1;
                    let hits: Vec<(String, BreachRecord)> = records
                        .into_iter()
                        .filter_map(|mut r| {
                            let domain = by_name.get(r.name.as_str())?;
                            r.confirmed_for_user = true;
                            Some((domain.to_string(), r))
                        })
                        .collect();
                    if !hits.is_empty() {
                        state.mutate(|d| merge::merge_breaches(d, hits))?;
                    }
                }
                Err(e @ (BreachError::InvalidKey | BreachError::RateLimited)) => {
                    // No point continuing: every further request would fail the same way.
                    lookup_error = Some(IpcError::from(e));
                    break;
                }
                Err(e) => {
                    log::debug!("Adresse nicht geprüft: {e}");
                    lookup_error.get_or_insert(IpcError::from(e));
                }
            }
            if i + 1 < addresses.len() {
                pause(ctx, api.pause)?;
            }
        }
        confirmed_new = state
            .read(confirmed_count)?
            .saturating_sub(confirmed_before) as u64;
    }

    let after = state.read(breach_count)?;
    let new_breaches = after.saturating_sub(before);
    let notifications = state.read(|d| d.settings.notifications)?;
    if notifications && (new_breaches > 0 || confirmed_new > 0) {
        let n = new_breaches.max(confirmed_new as usize);
        state.host.notify(
            "ausgeloggt",
            &format!("{n} neue Datenlecks bei deinen Accounts gefunden"),
        );
    }
    let accounts_with = state.read(|d| {
        d.accounts
            .iter()
            .filter(|a| !a.security.breaches.is_empty())
            .count()
    })?;
    let summary =
        format!("{after} Datenlecks bei {accounts_with} Accounts, {lookups} Adressen geprüft");
    let payload = json!({
        "catalogue": catalogue.len(), "newBreaches": new_breaches,
        "addressesChecked": lookups, "confirmedNew": confirmed_new,
        "hibpKey": !key.is_empty(),
    });
    match lookup_error {
        Some(e) if lookups == 0 && !addresses.is_empty() => {
            let mut o = JobOutcome::failed(summary, e);
            o.payload = Some(payload);
            Ok(o)
        }
        _ => Ok(JobOutcome::ok(summary).with_payload(payload)),
    }
}

// ---- catalog refresh ----------------------------------------------------------------

pub fn start_catalog_refresh(state: &AppState) -> IpcResult<String> {
    if !state.read(|d| d.settings.allow_catalog_updates)? {
        return Err(IpcError::invalid(
            "Katalog Aktualisierungen sind in den Einstellungen ausgeschaltet",
        ));
    }
    let st = state.clone();
    state.start_job("catalog_refresh", move |ctx| {
        ctx.progress(0, 1, "Lade den aktuellen Löschkatalog");
        let dir = st.cache_dir();
        let catalog: Catalog = Catalog::refresh(&dir).map_err(IpcError::from)?;
        ctx.check()?;
        let n = catalog.len();
        st.set_catalog(catalog);
        Ok(
            JobOutcome::ok(format!("Löschkatalog aktualisiert: {n} Dienste"))
                .with_payload(json!({ "entries": n })),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::unlocked;
    use ausgeloggt_core::model::{Account, MailboxConfig};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use uuid::Uuid;

    fn record(name: &str) -> BreachRecord {
        BreachRecord {
            name: name.into(),
            title: name.into(),
            breach_date: Some("2020-01-01".into()),
            data_classes: vec!["Email addresses".into()],
            confirmed_for_user: false,
        }
    }

    fn catalogue() -> Vec<CatalogBreach> {
        vec![
            CatalogBreach {
                domain: "adobe.com".into(),
                record: record("Adobe"),
                is_verified: true,
                is_sensitive: false,
            },
            CatalogBreach {
                domain: "".into(),
                record: record("NoDomain"),
                is_verified: true,
                is_sensitive: false,
            },
        ]
    }

    fn setup(key: &str) -> crate::state::testing::Fixture {
        let f = unlocked();
        let mut acc = Account::new("adobe.com", "Adobe");
        acc.identifiers.push("Erika@Example.org".into());
        let mut other = Account::new("spotify.com", "Spotify");
        other.identifiers.push("erika@example.org".into());
        f.state
            .mutate(|d| {
                d.accounts.extend([acc, other]);
                d.settings.hibp_api_key = key.into();
                d.mailboxes.push(MailboxConfig {
                    id: Uuid::new_v4(),
                    address: "erika@example.org".into(),
                    imap_host: "h".into(),
                    imap_port: 993,
                    username: "u".into(),
                    password: "p".into(),
                    folders: vec![],
                    progress: vec![],
                    last_scan: None,
                });
            })
            .unwrap();
        f
    }

    fn run(f: &crate::state::testing::Fixture, api: BreachApi) -> serde_json::Value {
        let st = f.state.clone();
        f.state
            .start_job("breach_check", move |ctx| breach_job(ctx, &st, &api))
            .unwrap();
        f.host.wait_for("job://done", 10).unwrap()
    }

    #[test]
    fn catalogue_only_without_key_makes_no_address_lookups() {
        let f = setup("");
        let calls = Arc::new(AtomicUsize::new(0));
        let c2 = calls.clone();
        let done = run(
            &f,
            BreachApi {
                catalogue: Box::new(|| Ok(catalogue())),
                for_address: Box::new(move |_, _| {
                    c2.fetch_add(1, Ordering::SeqCst);
                    Ok(vec![])
                }),
                pause: Duration::ZERO,
            },
        );
        assert_eq!(done["ok"], true, "{done}");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(done["payload"]["newBreaches"], 1);
        f.state
            .read(|d| {
                let adobe = d.accounts.iter().find(|a| a.domain == "adobe.com").unwrap();
                assert_eq!(adobe.security.breaches.len(), 1);
                assert!(!adobe.security.breaches[0].confirmed_for_user);
                let spot = d
                    .accounts
                    .iter()
                    .find(|a| a.domain == "spotify.com")
                    .unwrap();
                assert!(spot.security.breaches.is_empty());
            })
            .unwrap();
        assert!(f
            .host
            .notes
            .lock()
            .unwrap()
            .iter()
            .any(|(_, b)| b.contains("Datenlecks")));
    }

    #[test]
    fn key_triggers_deduplicated_lookups_and_confirms_hits() {
        let f = setup("KEY");
        let seen = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let s2 = seen.clone();
        let done = run(
            &f,
            BreachApi {
                catalogue: Box::new(|| Ok(catalogue())),
                for_address: Box::new(move |key, addr| {
                    assert_eq!(key, "KEY");
                    s2.lock().unwrap().push(addr.to_string());
                    Ok(vec![record("Adobe"), record("UnknownBreach")])
                }),
                pause: Duration::ZERO,
            },
        );
        assert_eq!(done["ok"], true, "{done}");
        // mailbox address + identifiers, lowercased and deduplicated -> exactly one lookup
        assert_eq!(*seen.lock().unwrap(), vec!["erika@example.org"]);
        assert_eq!(done["payload"]["addressesChecked"], 1);
        f.state
            .read(|d| {
                let adobe = d.accounts.iter().find(|a| a.domain == "adobe.com").unwrap();
                assert_eq!(adobe.security.breaches.len(), 1);
                assert!(adobe.security.breaches[0].confirmed_for_user);
            })
            .unwrap();
    }

    #[test]
    fn invalid_key_stops_lookups_and_reports_auth() {
        let f = setup("BAD");
        let calls = Arc::new(AtomicUsize::new(0));
        let c2 = calls.clone();
        let done = run(
            &f,
            BreachApi {
                catalogue: Box::new(|| Ok(catalogue())),
                for_address: Box::new(move |_, _| {
                    c2.fetch_add(1, Ordering::SeqCst);
                    Err(BreachError::InvalidKey)
                }),
                pause: Duration::ZERO,
            },
        );
        assert_eq!(done["ok"], false);
        assert_eq!(done["error"]["code"], "Auth");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn network_error_on_catalogue_fails_the_job() {
        let f = setup("");
        let done = run(
            &f,
            BreachApi {
                catalogue: Box::new(|| Err(BreachError::Network("offline".into()))),
                for_address: Box::new(|_, _| Ok(vec![])),
                pause: Duration::ZERO,
            },
        );
        assert_eq!(done["ok"], false);
        assert_eq!(done["error"]["code"], "Network");
    }

    #[test]
    fn cancel_during_pause_stops_lookups() {
        let f = setup("KEY");
        f.state
            .mutate(|d| {
                for i in 0..5 {
                    let mut a = Account::new(format!("s{i}.com"), "S");
                    a.identifiers.push(format!("u{i}@example.org"));
                    d.accounts.push(a);
                }
            })
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let c2 = calls.clone();
        let st = f.state.clone();
        let api = BreachApi {
            catalogue: Box::new(|| Ok(catalogue())),
            for_address: Box::new(move |_, _| {
                c2.fetch_add(1, Ordering::SeqCst);
                Ok(vec![])
            }),
            pause: Duration::from_secs(30),
        };
        let id = f
            .state
            .start_job("breach_check", move |ctx| breach_job(ctx, &st, &api))
            .unwrap();
        // wait for the first lookup, then cancel while it sleeps
        for _ in 0..100 {
            if calls.load(Ordering::SeqCst) >= 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        f.state.cancel_job(&id).unwrap();
        let done = f.host.wait_for("job://done", 5).unwrap();
        assert_eq!(done["summary"], "Abgebrochen");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn refresh_requires_permission() {
        let f = unlocked();
        f.state
            .mutate(|d| d.settings.allow_catalog_updates = false)
            .unwrap();
        assert!(start_catalog_refresh(&f.state).is_err());
    }
}
