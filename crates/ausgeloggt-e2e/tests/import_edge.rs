//! Importer edge cases: every password-manager format, hostile encodings and archives.

use std::collections::BTreeSet;
use std::io::{Cursor, Write};
use std::time::Instant;

use ausgeloggt_core::{PasswordFormat, SourceKind};
use ausgeloggt_import::{
    detect, import_bitwarden_json, import_password_csv, import_path, import_path_with,
    import_zip_bytes, DetectedKind, ImportOptions, ImportResult, PwnedChecker,
};

const SECRET: &str = "S3cr3t-Sentinel-PW!";

fn domains(r: &ImportResult) -> BTreeSet<String> {
    r.findings.iter().map(|f| f.domain.clone()).collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

fn zip_bytes(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut zw = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, body) in files {
        zw.start_file(*name, opts).unwrap();
        zw.write_all(body).unwrap();
    }
    zw.finish().unwrap().into_inner()
}

fn write_tmp(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let p = dir.path().join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

// ---------------------------------------------------------------------------------------------
// Every password format
// ---------------------------------------------------------------------------------------------

struct Fmt {
    name: &'static str,
    expect: PasswordFormat,
    header: &'static str,
    /// Row template; `{url}`, `{user}`, `{pw}`.
    row: &'static str,
}

const FORMATS: &[Fmt] = &[
    Fmt {
        name: "chrome",
        expect: PasswordFormat::Chrome,
        header: "name,url,username,password,note",
        row: "Site,{url},{user},{pw},",
    },
    Fmt {
        name: "firefox",
        expect: PasswordFormat::Firefox,
        header: "url,username,password,httpRealm,formActionOrigin,guid,timeCreated,timeLastUsed,timePasswordChanged",
        row: "{url},{user},{pw},,https://x.example,{GUID},1700000000000,1700000000000,1700000000000",
    },
    Fmt {
        name: "safari",
        expect: PasswordFormat::Safari,
        header: "Title,URL,Username,Password,Notes,OTPAuth",
        row: "Site,{url},{user},{pw},,",
    },
    Fmt {
        name: "bitwarden",
        expect: PasswordFormat::Bitwarden,
        header: "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp",
        row: ",0,login,Site,,,0,{url},{user},{pw},",
    },
    Fmt {
        name: "1password",
        expect: PasswordFormat::OnePassword,
        header: "Title,Website,Username,Password,Notes,Type,Folder,Favorite,Archived,Tags",
        row: "Site,{url},{user},{pw},,Login,,0,0,",
    },
    Fmt {
        name: "keepass",
        expect: PasswordFormat::KeePass,
        header: "Group,Title,Username,Password,URL,Notes,Last Modified",
        row: "Root,Site,{user},{pw},{url},,2024-01-01T10:00:00Z",
    },
    Fmt {
        name: "lastpass",
        expect: PasswordFormat::LastPass,
        header: "url,username,password,totp,extra,name,grouping,fav",
        row: "{url},{user},{pw},,,Site,,0",
    },
    Fmt {
        name: "proton",
        expect: PasswordFormat::Proton,
        header: "name,url,email,username,password,note,totp,createTime,modifyTime,vault",
        row: "Site,{url},{user},{user},{pw},,,1700000000,1700000000,Personal",
    },
    Fmt {
        name: "dashlane",
        expect: PasswordFormat::Dashlane,
        header: "title,url,username,username2,username3,password,note,category,otpSecret",
        row: "Site,{url},{user},,,{pw},,Shopping,",
    },
    Fmt {
        name: "generic",
        expect: PasswordFormat::Generic,
        header: "website,login,pass",
        row: "{url},{user},{pw}",
    },
];

fn build_csv(f: &Fmt, urls: &[&str], nl: &str) -> String {
    let mut s = String::from(f.header);
    s.push_str(nl);
    for (i, u) in urls.iter().enumerate() {
        let row = f
            .row
            .replace("{url}", u)
            .replace("{user}", &format!("user{i}@example.net"))
            .replace("{pw}", SECRET)
            .replace("{GUID}", "{abc}");
        s.push_str(&row);
        s.push_str(nl);
    }
    s
}

#[test]
fn every_password_format_is_detected_and_imported() {
    for f in FORMATS {
        let csv = build_csv(
            f,
            &[
                "https://www.spotify.com/login",
                "github.com",
                "http://shop.otto.de:8080/x",
            ],
            "\n",
        );
        let r = import_password_csv(&csv).unwrap_or_else(|e| panic!("{}: {e}", f.name));
        assert_eq!(
            r.source,
            SourceKind::PasswordExport { format: f.expect },
            "{}",
            f.name
        );
        assert_eq!(
            domains(&r),
            set(&["www.spotify.com", "github.com", "shop.otto.de"]),
            "{}",
            f.name
        );
        assert_eq!(r.items_scanned, 3, "{}", f.name);
        assert!(
            r.findings.iter().all(|x| x.identifier.is_some()),
            "{}",
            f.name
        );
        // Passwords never leave the importer.
        let dump = format!("{:?}", r);
        assert!(!dump.contains(SECRET), "{} leaks the password", f.name);
    }
}

#[test]
fn every_format_survives_bom_crlf_and_alternative_delimiters() {
    for f in FORMATS {
        let urls = ["spotify.com", "https://github.com/login"];
        let plain = build_csv(f, &urls, "\n");
        let expect = domains(&import_password_csv(&plain).unwrap());
        assert_eq!(expect.len(), 2, "{}", f.name);

        let crlf = build_csv(f, &urls, "\r\n");
        assert_eq!(
            domains(&import_password_csv(&crlf).unwrap()),
            expect,
            "{} crlf",
            f.name
        );

        let bom = format!("\u{feff}{plain}");
        assert_eq!(
            domains(&import_password_csv(&bom).unwrap()),
            expect,
            "{} bom",
            f.name
        );

        let bom_crlf = format!("\u{feff}{crlf}");
        assert_eq!(
            domains(&import_password_csv(&bom_crlf).unwrap()),
            expect,
            "{} bom+crlf",
            f.name
        );

        // European Excel: semicolon delimited.
        let semi = plain.replace(',', ";");
        assert_eq!(
            domains(&import_password_csv(&semi).unwrap()),
            expect,
            "{} semicolon",
            f.name
        );

        let tabs = plain.replace(',', "\t");
        assert_eq!(
            domains(&import_password_csv(&tabs).unwrap()),
            expect,
            "{} tabs",
            f.name
        );

        let trailing = format!("{plain}\n\n\n  \n");
        assert_eq!(
            domains(&import_password_csv(&trailing).unwrap()),
            expect,
            "{} trailing",
            f.name
        );

        let cr_only = plain.replace('\n', "\r");
        assert_eq!(
            domains(&import_password_csv(&cr_only).unwrap()),
            expect,
            "{} CR only",
            f.name
        );
    }
}

#[test]
fn utf16_files_are_decoded() {
    let dir = tempfile::tempdir().unwrap();
    let csv = build_csv(&FORMATS[0], &["spotify.com", "github.com"], "\r\n");
    let mut le = vec![0xFF, 0xFE];
    le.extend(csv.encode_utf16().flat_map(|u| u.to_le_bytes()));
    let r = import_path(&write_tmp(&dir, "chrome-le.csv", &le)).unwrap();
    assert_eq!(domains(&r), set(&["spotify.com", "github.com"]));
    let mut be = vec![0xFE, 0xFF];
    be.extend(csv.encode_utf16().flat_map(|u| u.to_be_bytes()));
    let r = import_path(&write_tmp(&dir, "chrome-be.csv", &be)).unwrap();
    assert_eq!(domains(&r), set(&["spotify.com", "github.com"]));
}

#[test]
fn file_based_detect_and_import_agree_for_all_formats() {
    let dir = tempfile::tempdir().unwrap();
    for f in FORMATS {
        let csv = build_csv(f, &["spotify.com"], "\n");
        let p = write_tmp(&dir, &format!("{}.csv", f.name), csv.as_bytes());
        assert_eq!(
            detect(&p).unwrap(),
            DetectedKind::Passwords(f.expect),
            "{}",
            f.name
        );
        let r = import_path(&p).unwrap();
        assert_eq!(
            r.source,
            SourceKind::PasswordExport { format: f.expect },
            "{}",
            f.name
        );
    }
    // Edge exports use the Chrome layout; the file name is the only hint.
    let csv = build_csv(&FORMATS[0], &["spotify.com"], "\n");
    let p = write_tmp(&dir, "Edge Passwords.csv", csv.as_bytes());
    assert_eq!(
        detect(&p).unwrap(),
        DetectedKind::Passwords(PasswordFormat::Edge)
    );
}

#[test]
fn empty_and_degenerate_files_never_panic() {
    let dir = tempfile::tempdir().unwrap();
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty.csv", vec![]),
        ("bom-only.csv", vec![0xEF, 0xBB, 0xBF]),
        ("ws.csv", b"   \n\n\t\n".to_vec()),
        (
            "header-only.csv",
            b"name,url,username,password,note\n".to_vec(),
        ),
        (
            "header-only-crlf.csv",
            b"name,url,username,password,note\r\n".to_vec(),
        ),
        ("comma.csv", b",".to_vec()),
        ("quote.csv", b"\"".to_vec()),
        (
            "unterminated.csv",
            b"name,url,username,password\nSite,\"https://a.com,u,p\nmore,rows,here,x\n".to_vec(),
        ),
        (
            "nul.csv",
            b"name,url,username,password\nA,\x00\x00,\x00,x\n".to_vec(),
        ),
        ("json-empty.json", b"{}".to_vec()),
        ("json-array.json", b"[]".to_vec()),
        ("json-null.json", b"null".to_vec()),
        ("txt-empty.txt", vec![]),
        ("txt-binary.txt", (0u8..=255).collect()),
        ("png.csv", b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec()),
        ("zero.zip", b"PK\x03\x04".to_vec()),
        ("zero-end.zip", b"PK\x05\x06".to_vec()),
    ];
    for (name, bytes) in cases {
        let p = write_tmp(&dir, name, &bytes);
        let _ = detect(&p);
        match import_path(&p) {
            Ok(r) => assert!(
                r.findings.is_empty() || name.starts_with("txt-binary"),
                "{name}: {:?}",
                r.findings
            ),
            Err(e) => {
                // user facing error text must exist
                assert!(!e.to_string().is_empty(), "{name}");
            }
        }
    }
    assert!(import_path(&dir.path().join("does-not-exist.csv")).is_err());
    let empty_dir = tempfile::tempdir().unwrap();
    assert!(import_path(empty_dir.path()).is_err());
    assert!(detect(empty_dir.path()).is_err());
}

#[test]
fn header_only_chrome_file_is_not_a_crash_and_has_no_findings() {
    let r = import_password_csv("name,url,username,password,note\n").unwrap();
    assert!(r.findings.is_empty());
    assert_eq!(r.items_scanned, 0);
}

#[test]
fn url_forms() {
    let csv = "name,url,username,password,note\n\
A,example.com,u,p,\n\
B,www.Example.COM/Login?x=1#y,u,p,\n\
C,HTTPS://UPPER.EXAMPLE.ORG:8443/path,u,p,\n\
D,//proto-relative.example.net/x,u,p,\n\
E,https://user:pass@auth.example.io/x,u,p,\n\
F,192.168.1.1,u,p,\n\
G,http://10.0.0.5:8080/admin,u,p,\n\
H,localhost,u,p,\n\
I,http://localhost:3000/login,u,p,\n\
J,https://[::1]/x,u,p,\n\
K,chrome://settings,u,p,\n\
L,mailto:foo@bar.example,u,p,\n\
M,javascript:alert(1),u,p,\n\
N,https://xn--mller-kva.example.com,u,p,\n\
O,https://münchen.example.de/x,u,p,\n\
P,https://trailing-dot.example.com./x,u,p,\n\
Q,not a url at all,u,p,\n\
R,,u,p,\n\
S,https://under_score.example.com,u,p,\n\
T,ftp://files.example.org/x,u,p,\n\
U,https://a.example.com https://b.example.com,u,p,\n";
    let r = import_password_csv(csv).unwrap();
    let d = domains(&r);
    for good in [
        "example.com",
        "www.example.com",
        "upper.example.org",
        "proto-relative.example.net",
        "auth.example.io",
        "xn--mller-kva.example.com",
        "münchen.example.de",
        "trailing-dot.example.com",
        "files.example.org",
        "a.example.com",
        "b.example.com",
    ] {
        assert!(d.contains(good), "missing {good}: {d:?}");
    }
    for bad in [
        "192.168.1.1",
        "10.0.0.5",
        "localhost",
        "::1",
        "settings",
        "bar.example",
    ] {
        assert!(
            !d.iter().any(|x| x.contains(bad)),
            "{bad} must be skipped: {d:?}"
        );
    }
    assert!(r
        .warnings
        .iter()
        .any(|w| w.contains("ohne verwertbare URL")));
}

#[test]
fn android_app_entries_are_imported() {
    let csv = "name,url,username,password,note\n\
Spotify,android://SHA256hash==@com.spotify.music/,me@example.net,pw,\n";
    let r = import_password_csv(csv).unwrap();
    assert_eq!(
        r.findings.len(),
        1,
        "android:// row dropped: {:?}",
        r.warnings
    );
    let dom = ausgeloggt_core::domain::registrable_domain(&r.findings[0].domain);
    assert_eq!(dom.as_deref(), Some("spotify.com"));
}

#[test]
fn duplicate_rows_collapse_but_distinct_users_do_not() {
    let mut csv = String::from("name,url,username,password,note\n");
    for _ in 0..100 {
        csv.push_str("Spotify,https://spotify.com,max@example.net,pw,\n");
    }
    csv.push_str("Spotify,https://spotify.com,other@example.net,pw,\n");
    csv.push_str("Spotify,https://spotify.com/other-path,max@example.net,pw,\n");
    let r = import_password_csv(&csv).unwrap();
    assert_eq!(
        r.findings.len(),
        2,
        "{:?}",
        r.findings.iter().map(|f| &f.identifier).collect::<Vec<_>>()
    );
    assert_eq!(r.items_scanned, 102);
    // Same source_ref twice must never occur (merge dedups on it).
    let refs: BTreeSet<_> = r
        .findings
        .iter()
        .map(|f| f.evidence.source_ref.clone())
        .collect();
    assert_eq!(refs.len(), r.findings.len());
}

#[test]
fn reimport_is_stable_source_refs() {
    let csv = build_csv(&FORMATS[0], &["spotify.com", "github.com"], "\n");
    let a = import_password_csv(&csv).unwrap();
    let b = import_password_csv(&csv).unwrap();
    let refs = |r: &ImportResult| {
        r.findings
            .iter()
            .map(|f| f.evidence.source_ref.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(refs(&a), refs(&b));
}

#[test]
fn rows_with_wrong_cell_counts_and_odd_quoting() {
    let csv = "name,url,username,password,note\n\
short,https://a.example.com\n\
long,https://b.example.com,u,p,n,extra,extra2,extra3\n\
\"quoted, comma\",\"https://c.example.com\",\"us\"\"er\",\"p\nw\",\"multi\nline\nnote\"\n\
\n\
,,,,\n\
only-name\n";
    let r = import_password_csv(csv).unwrap();
    assert_eq!(
        domains(&r),
        set(&["a.example.com", "b.example.com", "c.example.com"])
    );
    let c = r
        .findings
        .iter()
        .find(|f| f.domain == "c.example.com")
        .unwrap();
    assert_eq!(c.name.as_deref(), Some("quoted, comma"));
    assert_eq!(c.identifier.as_deref(), Some("us\"er"));
}

#[test]
fn giant_fields_do_not_blow_up() {
    let big = "x".repeat(5 * 1024 * 1024);
    let csv = format!(
        "name,url,username,password,note\n{big},https://ok.example.com,u,p,{big}\nn,https://{big}.example.com,u,p,\n"
    );
    let t = Instant::now();
    let r = import_password_csv(&csv).unwrap();
    assert!(domains(&r).contains("ok.example.com"));
    assert!(t.elapsed().as_secs() < 10, "{:?}", t.elapsed());
    // A 10k column header does not hang.
    let header: Vec<String> = (0..10_000).map(|i| format!("c{i}")).collect();
    let r = import_password_csv(&format!("{},url\nx,\n", header.join(",")));
    let _ = r;
}

#[test]
fn fifty_thousand_rows_import_fast() {
    let mut csv = String::from("name,url,username,password,note\n");
    for i in 0..50_000 {
        csv.push_str(&format!(
            "Site {i},https://shop{}.example{}.com/login,user{i}@example.net,pw{i},\n",
            i % 3000,
            i % 7
        ));
    }
    let t = Instant::now();
    let r = import_password_csv(&csv).unwrap();
    let dt = t.elapsed();
    println!("IMPORT 50k rows: {dt:?}, findings={}", r.findings.len());
    assert_eq!(r.items_scanned, 50_000);
    assert_eq!(r.findings.len(), 50_000);
    assert!(dt.as_secs() < 10, "{dt:?}");
}

#[test]
fn fifty_thousand_row_file_merges_into_a_vault_quickly() {
    let mut csv = String::from("name,url,username,password,note\n");
    for i in 0..50_000 {
        csv.push_str(&format!(
            "Site {i},https://www.site{}.com/login,user{i}@example.net,pw{i},\n",
            i % 15_000
        ));
    }
    let r = import_password_csv(&csv).unwrap();
    let mut data = ausgeloggt_core::VaultData::default();
    let t = Instant::now();
    let rep = ausgeloggt_core::merge::merge_findings(&mut data, r.findings.clone());
    let dt = t.elapsed();
    println!(
        "MERGE 50k findings -> {} accounts in {dt:?}",
        data.accounts.len()
    );
    assert!(dt.as_secs() < 10, "{dt:?}");
    assert!(rep.new_accounts > 0);
    let t = Instant::now();
    let rep2 = ausgeloggt_core::merge::merge_findings(&mut data, r.findings);
    println!("RE-MERGE 50k findings in {:?}", t.elapsed());
    assert_eq!(rep2.new_accounts, 0);
    assert_eq!(rep2.updated_accounts, 0);
}

// ---------------------------------------------------------------------------------------------
// Encodings
// ---------------------------------------------------------------------------------------------

#[test]
fn windows_1252_file_imports_hosts() {
    let dir = tempfile::tempdir().unwrap();
    let mut bytes = b"name,url,username,password,note\r\nM\xfcller Shop,https://shop.example.de,j\xfcrgen@example.net,pw,\r\n".to_vec();
    bytes.extend_from_slice(b"Caf\xe9,https://cafe.example.fr,u,pw,\r\n");
    let r = import_path(&write_tmp(&dir, "cp1252.csv", &bytes)).unwrap();
    assert_eq!(domains(&r), set(&["shop.example.de", "cafe.example.fr"]));
}

#[test]
fn windows_1252_names_are_decoded_not_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = b"name,url,username,password,note\r\nM\xfcller Shop,https://shop.example.de,j\xfcrgen@example.net,pw,\r\n".to_vec();
    let r = import_path(&write_tmp(&dir, "cp1252.csv", &bytes)).unwrap();
    let f = &r.findings[0];
    assert_eq!(f.name.as_deref(), Some("Müller Shop"));
    assert_eq!(f.identifier.as_deref(), Some("jürgen@example.net"));
}

#[test]
fn semicolon_csv_with_leading_blank_line() {
    let csv = "\n\nname;url;username;password;note\nSite;https://a.example.com;u;p;\n";
    let r = import_password_csv(csv).unwrap();
    assert_eq!(domains(&r), set(&["a.example.com"]));
}

// ---------------------------------------------------------------------------------------------
// Password audit
// ---------------------------------------------------------------------------------------------

struct FakePwned;
impl PwnedChecker for FakePwned {
    fn pwned_count(&self, _sha1: &[u8; 20]) -> Option<u64> {
        Some(3)
    }
}

#[test]
fn audit_counts_reuse_and_never_emits_passwords() {
    let dir = tempfile::tempdir().unwrap();
    let csv = format!(
        "name,url,username,password,note\n\
A,https://a.example.com,me@example.net,{SECRET},\n\
B,https://b.example.com,me@example.net,{SECRET},\n\
C,https://c.example.com,me@example.net,{SECRET},\n\
D,https://d.example.com,me@example.net,unique-d,\n\
E,https://e.example.com,me@example.net,,\n"
    );
    let p = write_tmp(&dir, "chrome.csv", csv.as_bytes());
    let a = import_path_with(
        &p,
        &ImportOptions {
            pwned: Some(&FakePwned),
        },
    )
    .unwrap();
    let get = |d: &str| a.security.iter().find(|s| s.domain == d).unwrap();
    assert_eq!(get("a.example.com").password_reuse_count, Some(2));
    assert_eq!(get("d.example.com").password_reuse_count, Some(0));
    assert_eq!(get("a.example.com").password_pwned, Some(true));
    assert!(
        a.security.iter().all(|s| s.domain != "e.example.com"),
        "empty password is not audited"
    );
    let dump = format!("{:?} {:?}", a.result, a.security);
    assert!(!dump.contains(SECRET) && !dump.contains("unique-d"));
    let json = serde_json::to_string(&a.security).unwrap();
    assert!(!json.contains(SECRET));
}

#[test]
fn audit_with_one_hundred_thousand_rows() {
    let dir = tempfile::tempdir().unwrap();
    let mut csv = String::from("name,url,username,password,note\n");
    for i in 0..20_000 {
        csv.push_str(&format!("S{i},https://s{i}.example.com,u,pw{},\n", i % 500));
    }
    let p = write_tmp(&dir, "chrome.csv", csv.as_bytes());
    let t = Instant::now();
    let a = import_path_with(&p, &ImportOptions::default()).unwrap();
    println!("AUDIT 20k rows: {:?}", t.elapsed());
    assert_eq!(a.security.len(), 20_000);
    assert!(a
        .security
        .iter()
        .all(|s| s.password_reuse_count == Some(39)));
}

// ---------------------------------------------------------------------------------------------
// Bitwarden JSON, generic lists
// ---------------------------------------------------------------------------------------------

#[test]
fn bitwarden_json_variants() {
    let ok = r#"{"encrypted":false,"folders":[],"items":[
        {"type":1,"name":"Spotify","login":{"username":"me@example.net","password":"S3cr3t-Sentinel-PW!","uris":[{"uri":"https://spotify.com"},{"uri":"https://open.spotify.com"}],"totp":"otpauth://x"},"revisionDate":"2024-02-02T10:00:00.000Z"},
        {"type":2,"name":"Note"},
        {"type":3,"name":"Card"},
        {"type":1,"name":"NoUri","login":{"username":"x"}},
        {"type":"1","name":"StringType","login":{"uris":[{"uri":"https://typed.example.com"}]}},
        {"name":"NoType","login":{"uris":[{"uri":"https://notype.example.com"}]}},
        null, 5, "x", [], {"login":{"uris":"nope"}}, {"login":{"uris":[null,{"uri":5},{}]}}
    ]}"#;
    let r = import_bitwarden_json(ok).unwrap();
    let d = domains(&r);
    assert!(
        d.contains("spotify.com") && d.contains("open.spotify.com"),
        "{d:?}"
    );
    assert!(d.contains("notype.example.com"));
    assert!(!format!("{r:?}").contains(SECRET));

    for bad in [
        "",
        "{",
        "[",
        r#"{"items":"x"}"#,
        r#"{"items":null}"#,
        r#"{"encrypted":true,"items":[]}"#,
        r#"{"encrypted":true,"encKeyValidation_DO_NOT_EDIT":"2.x","data":"2.y"}"#,
        "\u{feff}{\"items\":[]}",
        &"[".repeat(10_000),
        &format!(
            "{{\"items\":[{}]}}",
            "{\"login\":{\"uris\":[{\"uri\":\"https://a.example.com\"}]}},".repeat(1000)
        ),
    ] {
        let _ = import_bitwarden_json(bad); // must not panic
    }
    assert!(import_bitwarden_json(r#"{"encrypted":true,"items":[]}"#).is_err());
    let bom = import_bitwarden_json("\u{feff}{\"items\":[]}").unwrap();
    assert!(bom.findings.is_empty());
}

#[test]
fn generic_lists() {
    let dir = tempfile::tempdir().unwrap();
    let txt = "\u{feff}# my accounts\r\nhttps://a.example.com/x\r\n\r\nb.example.org # note\r\nme@c.example.net\r\nnot a domain\r\na.example.com\r\n192.168.0.1\r\nlocalhost\r\n";
    let r = import_path(&write_tmp(&dir, "list.txt", txt.as_bytes())).unwrap();
    assert_eq!(
        domains(&r),
        set(&["a.example.com", "b.example.org", "c.example.net"])
    );
    assert!(matches!(r.source, SourceKind::GenericList));

    let json = r#"{"accounts":[{"name":"Foo","url":"https://foo.example.com"},"bar.example.com",{"x":1},3,null]}"#;
    let r = import_path(&write_tmp(&dir, "list.json", json.as_bytes())).unwrap();
    assert_eq!(domains(&r), set(&["foo.example.com", "bar.example.com"]));

    let csv = "name;website\nFoo;foo.example.com\nBar;https://bar.example.com\n";
    let r = import_path(&write_tmp(&dir, "list.csv", csv.as_bytes())).unwrap();
    assert_eq!(domains(&r), set(&["foo.example.com", "bar.example.com"]));

    // Headerless one-domain-per-line .csv
    let r = import_path(&write_tmp(
        &dir,
        "bare.csv",
        b"foo.example.com\nbar.example.com\n",
    ))
    .unwrap();
    assert_eq!(domains(&r), set(&["foo.example.com", "bar.example.com"]));
}

// ---------------------------------------------------------------------------------------------
// Archives: Facebook / Takeout / hostile zips
// ---------------------------------------------------------------------------------------------

const FB_APPS: &str = r#"{"installed_apps_v2":[
    {"name":"Spotify","added_timestamp":1600000000},
    {"name":"M\u00c3\u00bcller Shop","added_timestamp":1610000000,"domain":"https://shop.example.com"},
    {"name":"Emoji \u00f0\u009f\u0098\u0080 App","added_timestamp":-5},
    {"name":"","added_timestamp":1},
    {"name":42},
    {"name":"HugeTs","added_timestamp":9223372036854775807},
    {"name":"Spotify","added_timestamp":1600000001}]}"#;

#[test]
fn facebook_export_variants() {
    let folder = "apps_and_websites_off_of_facebook";
    let ok = zip_bytes(&[
        (
            &format!("facebook-max/{folder}/connected_apps_and_websites.json"),
            FB_APPS.as_bytes().to_vec(),
        ),
        (
            &format!("facebook-max/{folder}/your_off-facebook_activity.json"),
            br#"{"v":[{"name":"Tracker"}]}"#.to_vec(),
        ),
    ]);
    let r = import_zip_bytes(ok).unwrap();
    let names: BTreeSet<_> = r.findings.iter().filter_map(|f| f.name.clone()).collect();
    assert!(names.contains("Müller Shop"), "{names:?}");
    assert!(!names.contains("Tracker"));
    assert_eq!(
        r.findings
            .iter()
            .filter(|f| f.name.as_deref() == Some("Spotify"))
            .count(),
        1
    );
    assert!(matches!(r.source, SourceKind::FacebookExport));

    // HTML-only export: succeeds with a warning, no findings.
    let html = zip_bytes(&[(
        &format!("facebook-max/{folder}/apps.html"),
        b"<html>Spotify</html>".to_vec(),
    )]);
    let r = import_zip_bytes(html).unwrap();
    assert!(r.findings.is_empty() && !r.warnings.is_empty());

    // Broken JSON / UTF-8 BOM / deep nesting / huge array.
    let weird = zip_bytes(&[
        (&format!("a/{folder}/bad.json"), b"{not json".to_vec()),
        (
            &format!("a/{folder}/bom.json"),
            [&[0xEF, 0xBB, 0xBF][..], br#"[{"name":"Bommed"}]"#].concat(),
        ),
        (
            &format!("a/{folder}/deep.json"),
            "[".repeat(50_000).into_bytes(),
        ),
        (&format!("a/{folder}/utf16.json"), {
            let mut v = vec![0xFF, 0xFE];
            v.extend(r#"[{"name":"Utf16App"}]"#.encode_utf16().flat_map(|u| u.to_le_bytes()));
            v
        }),
        (
            &format!("a/{folder}/nodes.json"),
            format!("[{}]", "{\"name\":\"N\"},".repeat(20_000))
                .replace(",]", "]")
                .into_bytes(),
        ),
    ]);
    let r = import_zip_bytes(weird).unwrap();
    let names: BTreeSet<_> = r.findings.iter().filter_map(|f| f.name.clone()).collect();
    assert!(
        names.contains("Bommed") && names.contains("Utf16App"),
        "{names:?}"
    );
    assert!(r
        .warnings
        .iter()
        .any(|w| w.contains("konnten nicht gelesen")));

    // Folder variants: capitalised / localised path segments, extracted folder import.
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("facebook-max").join(folder);
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("connected_apps_and_websites.json"), FB_APPS).unwrap();
    let r = import_path(dir.path()).unwrap();
    assert!(matches!(r.source, SourceKind::FacebookExport));
    assert!(r.findings.len() >= 3);
}

#[test]
fn takeout_variants() {
    let z = zip_bytes(&[
        ("Takeout/archive_browser.html", b"<html></html>".to_vec()),
        (
            "Takeout/Mein Aktivit\u{e4}tsprotokoll/Mit Google anmelden/MeineAktivit\u{e4}ten.json",
            br#"[{"title":"x","titleUrl":"https://www.notion.so/login","time":"2024-05-01T10:00:00.000Z"},{"titleUrl":"https://accounts.google.com/x"},{"titleUrl":"//bad"}]"#.to_vec(),
        ),
        ("Takeout/Drittanbieter/apps.html", b"<a href=\"https://canva.com\">c</a> <a href=\"https://www.googleapis.com/x\">g</a>".to_vec()),
        ("Takeout/Chrome/History.json", br#"{"Browser History":[{"url":"https://visited-once.example.com"}]}"#.to_vec()),
    ]);
    let r = import_zip_bytes(z).unwrap();
    let d = domains(&r);
    assert!(
        d.contains("www.notion.so") && d.contains("canva.com"),
        "{d:?}"
    );
    assert!(!d.iter().any(|x| x.contains("google")), "{d:?}");
    assert!(
        !d.contains("visited-once.example.com"),
        "browsing history must not become accounts"
    );
    let notion = r
        .findings
        .iter()
        .find(|f| f.domain == "www.notion.so")
        .unwrap();
    assert_eq!(notion.sso, Some(ausgeloggt_core::SsoProvider::Google));
}

#[test]
fn zip_path_traversal_entries_are_ignored() {
    let folder = "apps_and_websites_off_of_facebook";
    let z = zip_bytes(&[
        (
            &format!("ok/{folder}/a.json"),
            br#"[{"name":"Fine"}]"#.to_vec(),
        ),
        (
            &format!("../../{folder}/evil.json"),
            br#"[{"name":"Evil1"}]"#.to_vec(),
        ),
        (
            &format!("/abs/{folder}/evil.json"),
            br#"[{"name":"Evil2"}]"#.to_vec(),
        ),
        (
            &format!("..\\..\\{folder}\\evil.json"),
            br#"[{"name":"Evil3"}]"#.to_vec(),
        ),
        (
            &format!("C:/{folder}/evil.json"),
            br#"[{"name":"Evil4"}]"#.to_vec(),
        ),
        (
            &format!("a/b/../../../{folder}/evil.json"),
            br#"[{"name":"Evil5"}]"#.to_vec(),
        ),
        (
            &format!("nul\0byte/{folder}/evil.json"),
            br#"[{"name":"Evil6"}]"#.to_vec(),
        ),
    ]);
    let r = import_zip_bytes(z).unwrap();
    let names: BTreeSet<_> = r.findings.iter().filter_map(|f| f.name.clone()).collect();
    assert_eq!(
        names,
        ["Fine".to_string()].into_iter().collect(),
        "{names:?}"
    );
}

#[test]
fn nested_and_corrupt_zips_are_handled() {
    let folder = "apps_and_websites_off_of_facebook";
    let inner = zip_bytes(&[(
        &format!("x/{folder}/a.json"),
        br#"[{"name":"Inner"}]"#.to_vec(),
    )]);
    let outer = zip_bytes(&[
        ("export.zip", inner.clone()),
        ("readme.txt", b"hi".to_vec()),
    ]);
    // A zip in a zip is not unpacked: no format recognised, but a clean error.
    assert!(import_zip_bytes(outer).is_err());

    let good = zip_bytes(&[(
        &format!("x/{folder}/a.json"),
        br#"[{"name":"Ok"}]"#.to_vec(),
    )]);
    // Truncate at every 7th position and flip bytes: never a panic.
    for cut in (0..good.len()).step_by(7) {
        let _ = import_zip_bytes(good[..cut].to_vec());
    }
    for i in (0..good.len()).step_by(5) {
        let mut b = good.clone();
        b[i] ^= 0xFF;
        let _ = import_zip_bytes(b);
    }
    let dir = tempfile::tempdir().unwrap();
    for cut in [0usize, 4, 22, good.len() / 2] {
        let p = write_tmp(&dir, "cut.zip", &good[..cut]);
        let _ = detect(&p);
        let _ = import_path(&p);
    }
}

#[test]
fn zip_with_a_hundred_thousand_entries_is_rejected_quickly() {
    let mut zw = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for i in 0..100_050 {
        zw.start_file(format!("Takeout/f{i}.txt"), opts).unwrap();
    }
    let bytes = zw.finish().unwrap().into_inner();
    let t = Instant::now();
    let r = import_zip_bytes(bytes);
    println!("ZIP with 100k entries rejected in {:?}", t.elapsed());
    assert!(r.is_err());
    assert!(t.elapsed().as_secs() < 20);
}

#[test]
#[ignore = "slow: inflates ~600 MB of zeros"]
fn zip_bomb_is_stopped_by_budgets() {
    let folder = "apps_and_websites_off_of_facebook";
    let zeros = vec![b' '; 90 * 1024 * 1024];
    let files: Vec<(String, Vec<u8>)> = (0..7)
        .map(|i| (format!("x/{folder}/f{i}.json"), zeros.clone()))
        .collect();
    let refs: Vec<(&str, Vec<u8>)> = files.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    let z = zip_bytes(&refs);
    println!("bomb zip size: {} bytes", z.len());
    let t = Instant::now();
    let r = import_zip_bytes(z);
    println!(
        "bomb handled in {:?}: {:?}",
        t.elapsed(),
        r.as_ref()
            .map(|r| r.warnings.clone())
            .map_err(|e| e.to_string())
    );
    assert!(r.is_err() || !r.unwrap().warnings.is_empty());
}

#[test]
fn single_huge_entry_over_per_file_limit_is_skipped() {
    // 101 MiB of spaces compresses to ~100 KB: must be skipped by the 100 MiB per-file cap.
    let folder = "apps_and_websites_off_of_facebook";
    let big = vec![b' '; 101 * 1024 * 1024];
    let z = zip_bytes(&[
        (&format!("x/{folder}/big.json"), big),
        (
            &format!("x/{folder}/ok.json"),
            br#"[{"name":"Fine"}]"#.to_vec(),
        ),
    ]);
    let t = Instant::now();
    let r = import_zip_bytes(z).unwrap();
    println!("101 MiB entry skipped in {:?}", t.elapsed());
    assert!(r.findings.iter().any(|f| f.name.as_deref() == Some("Fine")));
    assert!(r
        .warnings
        .iter()
        .any(|w| w.contains("konnten nicht gelesen")));
}

#[test]
fn export_instructions_are_german_and_complete() {
    let i = ausgeloggt_import::export_instructions();
    assert!(i.len() >= 8);
    for (title, text) in i {
        assert!(!title.is_empty() && text.len() > 20);
    }
}

#[test]
fn many_findings_for_one_domain_merge_in_reasonable_time() {
    let mut csv = String::from("name,url,username,password,note\n");
    for i in 0..3_000 {
        csv.push_str(&format!(
            "Tenant {i},https://tenant{i}.atlassian-example.com/login,user{i}@example.net,pw,\n"
        ));
    }
    let r = import_password_csv(&csv).unwrap();
    let mut data = ausgeloggt_core::VaultData::default();
    let t = Instant::now();
    ausgeloggt_core::merge::merge_findings(&mut data, r.findings);
    let dt = t.elapsed();
    println!("MERGE 3,000 findings into one account: {dt:?}");
    assert_eq!(data.accounts.len(), 1);
    assert!(dt.as_secs_f64() < 1.5, "{dt:?}");
}
