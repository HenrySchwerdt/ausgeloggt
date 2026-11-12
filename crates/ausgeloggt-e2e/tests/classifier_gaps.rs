//! Subjects that clearly signal "I have an account here" but are currently classified as
//! weak `OtherMail`. Each test is `#[ignore]`d with the observed gap; run with `-- --ignored`.

use ausgeloggt_core::EvidenceKind;
use ausgeloggt_mail::classify::{classify, MailHeader};

fn kind(subject: &str) -> Option<EvidenceKind> {
    let h = MailHeader {
        from: "Foo <noreply@foo-service.com>".into(),
        subject: subject.into(),
        to: "me@gmx.de".into(),
        ..Default::default()
    };
    classify(&h).map(|c| c.kind)
}

fn strong(k: Option<EvidenceKind>) -> bool {
    matches!(
        k,
        Some(
            EvidenceKind::Welcome
                | EvidenceKind::Verification
                | EvidenceKind::PasswordReset
                | EvidenceKind::LoginAlert
                | EvidenceKind::TwoFactor
        )
    )
}

fn weak_ones(subjects: &[&str]) -> Vec<String> {
    subjects
        .iter()
        .filter(|s| !strong(kind(s)))
        .map(|s| format!("{s} -> {:?}", kind(s)))
        .collect()
}

#[test]
fn control_known_good_subjects() {
    assert!(strong(kind("Willkommen bei Foo")));
    assert!(strong(kind("Reset your password")));
    assert!(strong(kind("Neue Anmeldung bei deinem Foo-Konto")));
}

#[test]
fn password_changed_notices_are_account_evidence() {
    let w = weak_ones(&[
        "Dein Passwort wurde geändert",
        "Ihr Passwort wurde geändert",
        "Your password was changed",
        "Your password has been changed",
        "Votre mot de passe a été modifié",
    ]);
    assert!(w.is_empty(), "{w:#?}");
}

#[test]
fn email_confirmed_notices_are_account_evidence() {
    let w = weak_ones(&[
        "Email address confirmed",
        "Thanks for verifying your email",
        "E-Mail-Adresse erfolgreich bestätigt",
        "Anmeldung bestätigt",
    ]);
    assert!(w.is_empty(), "{w:#?}");
}

#[test]
fn login_links_are_account_evidence() {
    let w = weak_ones(&[
        "Your Foo login link",
        "Magic link to sign in",
        "Login-Link für Foo",
        "Dein Einmal-Link zum Anmelden",
    ]);
    assert!(w.is_empty(), "{w:#?}");
}

#[test]
fn welcome_variants_are_account_evidence() {
    let w = weak_ones(&[
        "Merci d'avoir rejoint Foo",
        "Active ton compte",
        "You're in!",
        "Your account is ready",
        "Dein neues Konto ist startklar",
        "Aktivierung deines Kontos",
        "Schön, dass du da bist",
    ]);
    assert!(w.is_empty(), "{w:#?}");
}
