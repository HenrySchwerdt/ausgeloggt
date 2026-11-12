//! Send rate limiting per mailbox: a minimum gap between two mails and a daily cap.
//! The counters live in the vault (`VaultData::send_counters`), so a restart does not
//! reset them. "Day" is the local calendar day. Pure functions, time and zone injected.

use std::time::Duration;

use ausgeloggt_core::model::{MailboxConfig, SendCounter, Settings, VaultData};
use chrono::{DateTime, TimeZone, Utc};
use uuid::Uuid;

/// Cap per mailbox and day when the settings leave it at "automatic".
pub const DEFAULT_DAILY_CAP: u32 = 40;
/// Gmail tolerates more.
pub const GMAIL_DAILY_CAP: u32 = 100;
/// Upper bound for the configurable gap.
pub const MAX_GAP_SECONDS: u32 = 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// A mail may go out now.
    Ready,
    /// The minimum gap has not passed yet.
    Wait(Duration),
    /// The daily cap is used up.
    CapReached { cap: u32, sent: u32 },
}

pub fn effective_cap(settings: &Settings, mailbox: &MailboxConfig) -> u32 {
    if settings.daily_send_cap > 0 {
        settings.daily_send_cap
    } else if mailbox
        .imap_host
        .trim()
        .eq_ignore_ascii_case("imap.gmail.com")
    {
        GMAIL_DAILY_CAP
    } else {
        DEFAULT_DAILY_CAP
    }
}

pub fn gap(settings: &Settings) -> Duration {
    Duration::from_secs(u64::from(settings.send_gap_seconds.min(MAX_GAP_SECONDS)))
}

/// Local calendar day as `YYYY-MM-DD`.
pub fn day_key<Tz: TimeZone>(now: DateTime<Utc>, tz: &Tz) -> String {
    now.with_timezone(tz).date_naive().to_string()
}

pub fn sent_today<Tz: TimeZone>(
    data: &VaultData,
    mailbox_id: Uuid,
    now: DateTime<Utc>,
    tz: &Tz,
) -> u32 {
    let today = day_key(now, tz);
    data.send_counters
        .iter()
        .find(|c| c.mailbox_id == mailbox_id && c.day == today)
        .map_or(0, |c| c.count)
}

pub fn remaining_today<Tz: TimeZone>(
    data: &VaultData,
    mailbox: &MailboxConfig,
    now: DateTime<Utc>,
    tz: &Tz,
) -> u32 {
    effective_cap(&data.settings, mailbox).saturating_sub(sent_today(data, mailbox.id, now, tz))
}

pub fn gate<Tz: TimeZone>(
    data: &VaultData,
    mailbox: &MailboxConfig,
    now: DateTime<Utc>,
    tz: &Tz,
) -> Gate {
    let cap = effective_cap(&data.settings, mailbox);
    let sent = sent_today(data, mailbox.id, now, tz);
    if sent >= cap {
        return Gate::CapReached { cap, sent };
    }
    let min_gap = gap(&data.settings);
    let last = data
        .send_counters
        .iter()
        .find(|c| c.mailbox_id == mailbox.id)
        .and_then(|c| c.last_sent);
    if let (Some(last), false) = (last, min_gap.is_zero()) {
        let elapsed = (now - last).to_std().unwrap_or(Duration::ZERO);
        // A clock that jumped back must not block for longer than the gap itself.
        if elapsed < min_gap {
            return Gate::Wait(min_gap - elapsed);
        }
    }
    Gate::Ready
}

/// Count one sent mail.
pub fn record<Tz: TimeZone>(data: &mut VaultData, mailbox_id: Uuid, now: DateTime<Utc>, tz: &Tz) {
    let today = day_key(now, tz);
    match data
        .send_counters
        .iter_mut()
        .find(|c| c.mailbox_id == mailbox_id)
    {
        Some(c) => {
            if c.day == today {
                c.count += 1;
            } else {
                c.day = today;
                c.count = 1;
            }
            c.last_sent = Some(now);
        }
        None => data.send_counters.push(SendCounter {
            mailbox_id,
            day: today,
            count: 1,
            last_sent: Some(now),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn mb(host: &str) -> MailboxConfig {
        MailboxConfig {
            id: Uuid::new_v4(),
            address: "me@example.org".into(),
            imap_host: host.into(),
            imap_port: 993,
            username: "u".into(),
            password: "p".into(),
            folders: vec![],
            progress: vec![],
            last_scan: None,
        }
    }

    fn t(h: u32, m: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 12, h, m, s).unwrap()
    }

    #[test]
    fn caps_by_provider_and_setting() {
        let mut s = Settings::default();
        assert_eq!(effective_cap(&s, &mb("imap.web.de")), 40);
        assert_eq!(effective_cap(&s, &mb("IMAP.gmail.com")), 100);
        s.daily_send_cap = 7;
        assert_eq!(effective_cap(&s, &mb("imap.gmail.com")), 7);
    }

    #[test]
    fn gap_is_enforced_and_expires() {
        let mut d = VaultData::default();
        let m = mb("imap.web.de");
        assert_eq!(gate(&d, &m, t(10, 0, 0), &Utc), Gate::Ready);
        record(&mut d, m.id, t(10, 0, 0), &Utc);
        assert_eq!(
            gate(&d, &m, t(10, 0, 5), &Utc),
            Gate::Wait(Duration::from_secs(15))
        );
        assert_eq!(gate(&d, &m, t(10, 0, 20), &Utc), Gate::Ready);
        // a zero gap never waits
        d.settings.send_gap_seconds = 0;
        assert_eq!(gate(&d, &m, t(10, 0, 0), &Utc), Gate::Ready);
    }

    #[test]
    fn clock_going_backwards_waits_at_most_the_gap() {
        let mut d = VaultData::default();
        let m = mb("imap.web.de");
        record(&mut d, m.id, t(10, 0, 0), &Utc);
        match gate(&d, &m, t(9, 0, 0), &Utc) {
            Gate::Wait(w) => assert!(w <= Duration::from_secs(20)),
            Gate::Ready => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn cap_blocks_and_resets_the_next_local_day() {
        let mut d = VaultData::default();
        d.settings.send_gap_seconds = 0;
        d.settings.daily_send_cap = 3;
        let m = mb("imap.web.de");
        for i in 0..3 {
            assert_eq!(gate(&d, &m, t(10, i, 0), &Utc), Gate::Ready);
            record(&mut d, m.id, t(10, i, 0), &Utc);
        }
        assert_eq!(
            gate(&d, &m, t(11, 0, 0), &Utc),
            Gate::CapReached { cap: 3, sent: 3 }
        );
        assert_eq!(remaining_today(&d, &m, t(11, 0, 0), &Utc), 0);
        assert_eq!(
            gate(&d, &m, t(23, 59, 0), &Utc),
            Gate::CapReached { cap: 3, sent: 3 }
        );
        let next = Utc.with_ymd_and_hms(2026, 11, 13, 0, 1, 0).unwrap();
        assert_eq!(gate(&d, &m, next, &Utc), Gate::Ready);
        record(&mut d, m.id, next, &Utc);
        assert_eq!(sent_today(&d, m.id, next, &Utc), 1);
        assert_eq!(d.send_counters.len(), 1);
    }

    #[test]
    fn day_follows_the_local_zone() {
        let mut d = VaultData::default();
        d.settings.send_gap_seconds = 0;
        d.settings.daily_send_cap = 1;
        let m = mb("imap.web.de");
        let berlin = FixedOffset::east_opt(3600).unwrap();
        // 23:30 UTC on the 12th is 00:30 on the 13th in Berlin (winter time).
        record(&mut d, m.id, t(23, 30, 0), &berlin);
        assert_eq!(d.send_counters[0].day, "2026-11-13");
        // 00:10 UTC on the 13th is still the 13th in Berlin: same day, cap reached.
        let after = Utc.with_ymd_and_hms(2026, 11, 13, 0, 10, 0).unwrap();
        assert!(matches!(
            gate(&d, &m, after, &berlin),
            Gate::CapReached { .. }
        ));
        // The same instant belongs to the 12th in UTC.
        assert_eq!(day_key(t(23, 30, 0), &Utc), "2026-11-12");
    }

    #[test]
    fn counters_are_per_mailbox() {
        let mut d = VaultData::default();
        d.settings.daily_send_cap = 1;
        d.settings.send_gap_seconds = 0;
        let (a, b) = (mb("h1"), mb("h2"));
        record(&mut d, a.id, t(8, 0, 0), &Utc);
        assert!(matches!(
            gate(&d, &a, t(8, 1, 0), &Utc),
            Gate::CapReached { .. }
        ));
        assert_eq!(gate(&d, &b, t(8, 1, 0), &Utc), Gate::Ready);
    }

    #[test]
    fn counters_survive_a_vault_round_trip() {
        let mut d = VaultData::default();
        let m = mb("h");
        record(&mut d, m.id, t(8, 0, 0), &Utc);
        let json = serde_json::to_string(&d).unwrap();
        let back: VaultData = serde_json::from_str(&json).unwrap();
        assert_eq!(sent_today(&back, m.id, t(9, 0, 0), &Utc), 1);
        // an old vault without the new fields still loads
        let old: VaultData = serde_json::from_str(r#"{"schema":1,"accounts":[]}"#).unwrap();
        assert!(old.send_counters.is_empty() && old.automations.is_empty());
    }
}
