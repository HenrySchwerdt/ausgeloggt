// Demo data on top of the base accounts: payments, security, requests, catalogue, reference data, insights, letters.
// Everything is fake.
import type {
  Account, AliasAdvice, Authority, BillingInterval, BillingSummary, CatalogInfo, DataRequest, DeletionInfo, InsightsDto,
  LetterArgs, LetterPreview, Organization, Party, PaymentMethod, PaymentRecord, PrivacyScore, ProviderPreset,
  ReferenceData, RiskAssessment, RiskReason, SecurityInfo, Snapshot, Subscription,
} from "../types";
import { difficultyFor, deletionUrl, NOTES } from "./data";

const DAY = 86_400_000;
const now = () => Date.now();
const ago = (d: number) => new Date(now() - d * DAY).toISOString();
const inDays = (d: number) => new Date(now() + d * DAY).toISOString();
const uid = (n: number, p: string) => `${p}${n.toString(16).padStart(7, "0")}-0000-4000-8000-${(n * 7919).toString(16).padStart(12, "0")}`;

// ---------------------------------------------------------------------------------------------
// Payments
// ---------------------------------------------------------------------------------------------

const VISA: PaymentMethod = { Card: { brand: "Visa", last4: "1234" } };
const MC: PaymentMethod = { Card: { brand: "Mastercard", last4: "4821" } };
const SEPA: PaymentMethod = { Sepa: { iban_last4: "4432" } };

interface PaySpec {
  plan: string; amount: number; interval: BillingInterval | null; method: PaymentMethod; via?: string;
  /** days until the next charge (active subscriptions) */
  next?: number; charges?: number; increased?: boolean; cancelled?: number; unused?: boolean;
}
const CYCLE: Record<BillingInterval, number> = { Weekly: 7, BiWeekly: 14, BiMonthly: 61, FourMonthly: 122, Monthly: 30, Quarterly: 91, HalfYearly: 182, Yearly: 365 };
const PER_YEAR: Record<BillingInterval, number> = { Weekly: 52, BiWeekly: 26, BiMonthly: 6, FourMonthly: 3, Monthly: 12, Quarterly: 4, HalfYearly: 2, Yearly: 1 };

const PAY: Record<string, PaySpec> = {
  "spotify.com": { plan: "Premium Individual", amount: 1099, interval: "Monthly", method: VISA, next: 6, charges: 24 },
  "netflix.com": { plan: "Standard", amount: 1799, interval: "Monthly", method: MC, next: 12, charges: 36, increased: true },
  "amazon.de": { plan: "Prime", amount: 8990, interval: "Yearly", method: { Other: "PayPal" } as PaymentMethod, next: 94, charges: 4 },
  "adobe.com": { plan: "Creative Cloud", amount: 2379, interval: "Monthly", method: VISA, charges: 14, cancelled: 40 },
  "dropbox.com": { plan: "Plus", amount: 11988, interval: "Yearly", method: "PayPal", next: 21, charges: 4 },
  "notion.so": { plan: "Plus", amount: 1000, interval: "Monthly", method: VISA, next: 17, charges: 10 },
  "audible.de": { plan: "Monatsabo", amount: 995, interval: "Monthly", method: VISA, next: 3, charges: 18, unused: true },
  "hellofresh.de": { plan: "Kochbox", amount: 3499, interval: "Monthly", method: MC, charges: 6, cancelled: 21 },
  "disneyplus.com": { plan: "Standard", amount: 899, interval: "Monthly", method: "ApplePay", via: "Apple", next: 9, charges: 7 },
  "canva.com": { plan: "Pro", amount: 11999, interval: "Yearly", method: "GooglePay", via: "Google Play", next: 140, charges: 2 },
  "strava.com": { plan: "Abo", amount: 5999, interval: "Yearly", method: "ApplePay", via: "Apple", next: 201, charges: 2 },
  "zoom.us": { plan: "Pro", amount: 1399, interval: "Monthly", method: VISA, next: 25, charges: 11, unused: true },
  "sky.de": { plan: "Entertainment", amount: 2999, interval: "Monthly", method: SEPA, next: 14, charges: 30, increased: true, unused: true },
  "duolingo.com": { plan: "Super", amount: 799, interval: "Monthly", method: "ApplePay", via: "Apple", next: 2, charges: 5 },
  "linkedin.com": { plan: "Premium Career", amount: 3999, interval: "Monthly", method: VISA, next: 27, charges: 3, unused: true },
  "udemy.com": { plan: "", amount: 1299, interval: null, method: VISA, charges: 2 },
  "zalando.de": { plan: "", amount: 8995, interval: null, method: "Klarna", charges: 3 },
  "ebay.de": { plan: "", amount: 2450, interval: null, method: "PayPal", charges: 2 },
  "otto.de": { plan: "", amount: 15990, interval: null, method: "Invoice", charges: 1 },
  "steampowered.com": { plan: "", amount: 2999, interval: null, method: VISA, charges: 4 },
  "epicgames.com": { plan: "", amount: 1999, interval: null, method: MC, charges: 2 },
  "vinted.de": { plan: "", amount: 1200, interval: null, method: "PayPal", charges: 2 },
  "etsy.com": { plan: "", amount: 3400, interval: null, method: "PayPal", charges: 2 },
  "booking.com": { plan: "", amount: 24800, interval: null, method: VISA, charges: 1 },
  "airbnb.de": { plan: "", amount: 31200, interval: null, method: VISA, charges: 1 },
  "lieferando.de": { plan: "", amount: 2140, interval: null, method: VISA, charges: 5 },
  "uber.com": { plan: "", amount: 1480, interval: null, method: VISA, charges: 3 },
  "ikea.com": { plan: "", amount: 12900, interval: null, method: VISA, charges: 1 },
  "mediamarkt.de": { plan: "", amount: 49900, interval: null, method: MC, charges: 1 },
  "bahn.de": { plan: "", amount: 8990, interval: null, method: SEPA, charges: 3 },
};
PAY["amazon.de"].method = "PayPal";

function paymentsFor(spec: PaySpec): PaymentRecord[] {
  const out: PaymentRecord[] = [];
  const cycle = spec.interval ? CYCLE[spec.interval] : 70;
  const lastAgo = spec.cancelled !== undefined ? spec.cancelled + 3 : spec.interval ? Math.max(1, cycle - (spec.next ?? cycle)) : 24;
  const n = spec.charges ?? 1;
  for (let k = 0; k < n; k++) {
    const old = spec.increased && k >= 3;
    const amount = old ? Math.round(spec.amount * 0.88) : spec.amount;
    out.push({
      date: ago(lastAgo + k * cycle), kind: spec.interval ? "Renewal" : "Charge", amount_minor: amount, currency: "EUR",
      plan: spec.plan || null, method: spec.method, via: spec.via ?? null,
      summary: spec.interval ? `${spec.plan}, Verlängerung` : "Bestellung", source_ref: `pay-${k}`,
    });
  }
  if (spec.increased) {
    out.push({
      date: ago(lastAgo + 5), kind: "PriceChange", amount_minor: spec.amount, currency: "EUR", plan: spec.plan,
      method: spec.method, via: spec.via ?? null, summary: "Preisänderung angekündigt", source_ref: "pay-pc",
    });
  }
  if (spec.cancelled !== undefined) {
    out.push({
      date: ago(spec.cancelled), kind: "Cancelled", amount_minor: null, currency: null, plan: spec.plan,
      method: null, via: null, summary: "Kündigung bestätigt", source_ref: "pay-c",
    });
  }
  return out;
}

// ---------------------------------------------------------------------------------------------
// Security
// ---------------------------------------------------------------------------------------------

const SEC: Record<string, Partial<SecurityInfo>> = {
  "spotify.com": { password_reuse_count: 3, password_pwned: false, has_2fa: false },
  "amazon.de": { password_reuse_count: 0, password_pwned: false, has_2fa: true },
  "adobe.com": { password_reuse_count: 2, password_pwned: true, has_2fa: false },
  "dropbox.com": { password_reuse_count: 4, password_pwned: false, has_2fa: false },
  "slack.com": { password_reuse_count: 0, password_pwned: true, has_2fa: false },
  "reddit.com": { password_reuse_count: 3, password_pwned: false, has_2fa: false },
  "ebay.de": { password_reuse_count: 4, password_pwned: false, has_2fa: false },
  "linkedin.com": { password_reuse_count: 0, password_pwned: true, has_2fa: false },
  "udemy.com": { password_reuse_count: 2, password_pwned: false, has_2fa: null },
  "zoom.us": { password_reuse_count: 3, password_pwned: false, has_2fa: false },
  "vinted.de": { password_reuse_count: 0, password_pwned: false, has_2fa: null },
  "github.com": { password_reuse_count: 0, password_pwned: false, has_2fa: true },
  "paypal.com": { password_reuse_count: 0, password_pwned: false, has_2fa: true },
  "google.com": { password_reuse_count: 0, password_pwned: false, has_2fa: true },
  "apple.com": { password_reuse_count: 0, password_pwned: false, has_2fa: true },
  "revolut.com": { password_reuse_count: 0, password_pwned: false, has_2fa: true },
  "wise.com": { password_reuse_count: 0, password_pwned: false, has_2fa: true },
  "steampowered.com": { password_reuse_count: 0, password_pwned: false, has_2fa: true },
  "microsoft.com": { password_reuse_count: 1, password_pwned: false, has_2fa: true },
  "notion.so": { password_reuse_count: 0, password_pwned: false, has_2fa: false },
  "twitch.tv": { has_2fa: false },
  "x.com": { has_2fa: false },
  "instagram.com": { has_2fa: false },
  "discord.com": { has_2fa: false },
  "whatsapp.com": { phone_login: true, has_2fa: true },
  "tinder.com": { phone_login: true },
  "snapchat.com": { phone_login: true },
};
const BREACH: Record<string, { name: string; title: string; date: string; classes: string[]; mine: boolean }> = {
  "adobe.com": { name: "Adobe", title: "Adobe", date: "2013-10-04", classes: ["Mailadressen", "Passworthinweise", "Passwörter"], mine: true },
  "linkedin.com": { name: "LinkedIn", title: "LinkedIn", date: "2012-05-05", classes: ["Mailadressen", "Passwörter"], mine: true },
  "dropbox.com": { name: "Dropbox", title: "Dropbox", date: "2012-07-01", classes: ["Mailadressen", "Passwörter"], mine: true },
  "canva.com": { name: "Canva", title: "Canva", date: "2019-05-24", classes: ["Mailadressen", "Namen", "Passwörter"], mine: false },
  "twitch.tv": { name: "Twitch", title: "Twitch", date: "2021-10-06", classes: ["Quellcode", "Auszahlungen"], mine: false },
  "tumblr.com": { name: "Tumblr", title: "Tumblr", date: "2013-02-28", classes: ["Mailadressen", "Passwörter"], mine: false },
  "deezer.com": { name: "Deezer", title: "Deezer", date: "2019-11-01", classes: ["Mailadressen", "Geburtsdaten", "IP Adressen"], mine: false },
  "trello.com": { name: "Trello", title: "Trello", date: "2024-01-21", classes: ["Mailadressen", "Namen"], mine: true },
};
const SUPPORTS_2FA = ["dropbox.com", "slack.com", "reddit.com", "linkedin.com", "twitch.tv", "x.com", "instagram.com", "notion.so", "discord.com", "ebay.de", "zoom.us", "epicgames.com", "adobe.com", "canva.com", "figma.com", "github.com", "amazon.de", "paypal.com", "google.com", "apple.com", "steampowered.com", "revolut.com", "wise.com", "microsoft.com", "airbnb.de", "booking.com", "tiktok.com", "snapchat.com", "udemy.com", "spotify.com"];
SUPPORTS_2FA.splice(SUPPORTS_2FA.indexOf("spotify.com"), 1);
const FOREIGN = new Set(["slack.com", "zoom.us", "figma.com", "miro.com", "asana.com"]);

export function enrichAccounts(accounts: Account[]): Account[] {
  return accounts.map((a) => {
    const spec = PAY[a.domain];
    const sec = SEC[a.domain];
    const br = BREACH[a.domain];
    const security: SecurityInfo = {
      password_reuse_count: null, password_pwned: null, has_2fa: null, phone_login: false, breaches: [], checked_at: null,
      ...sec,
    };
    if (sec || br) security.checked_at = ago(3);
    if (br) {
      security.breaches = [{ name: br.name, title: br.title, breach_date: br.date, data_classes: br.classes, confirmed_for_user: br.mine }];
    }
    const identifiers = FOREIGN.has(a.domain) ? [...a.identifiers, "lena.hartmann@kreativwerk-agentur.de"] : a.identifiers;
    return { ...a, security, payments: spec ? paymentsFor(spec) : [], identifiers };
  });
}

// ---------------------------------------------------------------------------------------------
// Requests and subscriptions
// ---------------------------------------------------------------------------------------------

export function buildRequests(accounts: Account[], subs: Subscription[]): DataRequest[] {
  const acc = (d: string) => accounts.find((a) => a.domain === d);
  const sub = (d: string) => subs.find((s) => s.domain === d);
  const mk = (n: number, r: Partial<DataRequest> & Pick<DataRequest, "kind" | "target" | "subject" | "status">, sentDays: number | null, deadlineIn: number | null): DataRequest => ({
    id: uid(n, "d"), domain: null, account_id: null, subscription_id: null, parent_id: null, channel: "Email",
    recipient: "", body: "", created_at: ago((sentDays ?? 0) + 0.2), sent_at: sentDays === null ? null : ago(sentDays),
    deadline: deadlineIn === null ? null : inDays(deadlineIn), notes: "", ...r,
  });
  const body = (what: string, name: string) =>
    `Sehr geehrte Damen und Herren,\n\nhiermit beantrage ich ${what} bei ${name}.\n\nMit freundlichen Grüßen\nLena Hartmann`;
  const out: DataRequest[] = [];
  const a = acc("adobe.com"), x = acc("x.com"), z = acc("zalando.de"), b = acc("booking.com"), o = acc("otto.de"), am = acc("amazon.de");
  if (a) out.push(mk(1, { kind: "Erasure", target: "Adobe", domain: "adobe.com", account_id: a.id, recipient: "privacy@adobe.com", subject: "Antrag auf Löschung (Art. 17 DSGVO), Adobe", body: body("die Löschung meiner Daten", "Adobe"), status: "Sent" }, 34, -4));
  if (x) out.push(mk(2, { kind: "Erasure", target: "X", domain: "x.com", account_id: x.id, recipient: "privacy@x.com", subject: "Antrag auf Löschung (Art. 17 DSGVO), X", body: body("die Löschung meiner Daten", "X"), status: "Sent" }, 41, -11));
  if (z) out.push(mk(3, { kind: "Erasure", target: "Zalando", domain: "zalando.de", account_id: z.id, recipient: "datenschutz@zalando.de", subject: "Antrag auf Löschung (Art. 17 DSGVO), Zalando", body: body("die Löschung meiner Daten", "Zalando"), status: "Sent" }, 12, 18));
  if (b) out.push(mk(4, { kind: "Access", target: "Booking.com", domain: "booking.com", account_id: b.id, recipient: "datenschutz@booking.com", subject: "Auskunft nach Art. 15 DSGVO, Booking.com", body: body("Auskunft über meine Daten", "Booking.com"), status: "Sent" }, 5, 25));
  if (am) out.push(mk(5, { kind: "Cancellation", target: "Amazon", domain: "amazon.de", account_id: am.id, recipient: "kuendigung@amazon.de", subject: "Kündigung Prime", body: body("die Kündigung meiner Prime Mitgliedschaft", "Amazon"), status: "Draft" }, null, null));
  if (o) out.push(mk(6, { kind: "Erasure", target: "Otto", domain: "otto.de", account_id: o.id, recipient: "datenschutz@otto.de", subject: "Antrag auf Löschung (Art. 17 DSGVO), Otto", body: body("die Löschung meiner Daten", "Otto"), status: "Answered" }, 70, null));
  out.push(mk(7, { kind: "CreditBureauAccess", target: "SCHUFA Holding AG", recipient: "SCHUFA Holding AG\nKormoranweg 5\n65201 Wiesbaden", channel: "Post", subject: "Datenkopie nach Art. 15 DSGVO", body: body("eine Datenkopie", "der SCHUFA"), status: "Sent" }, 20, 10));
  const d = sub("duolingo.com");
  if (d) out.push(mk(8, { kind: "CeaseAndDesist", target: "Duolingo", domain: "duolingo.com", subscription_id: d.id, recipient: "legal@duolingo.com", subject: "Unterlassung von Werbemails", body: body("die Unterlassung weiterer Werbemails", "Duolingo"), status: "Sent" }, 3, 11));
  return out;
}

export function withPostUnsubscribeMails(subs: Subscription[]): Subscription[] {
  return subs.map((s) => {
    if (s.status !== "StillReceiving") return s;
    const subjects = s.sample_subjects.length ? s.sample_subjects : ["Newsletter"];
    return {
      ...s,
      history: [{ at: ago(24), method: s.unsubscribe[0] ?? { Link: { url: `https://${s.domain}` } }, ok: true, message: "Abmeldung bestätigt (HTTP 200)" }],
      post_unsubscribe_mails: [3, 9, 17].map((d, i) => ({
        date: ago(d), from: s.sender, subject: subjects[i % subjects.length]!, message_id: `<${i}${s.id}@${s.domain}>`, raw_headers: "",
      })),
    };
  });
}

// ---------------------------------------------------------------------------------------------
// Catalogue and reference data
// ---------------------------------------------------------------------------------------------

/** Services that only take requests by post (no mail address known). */
const POST_ONLY = ["reddit.com", "etsy.com"];

export function catalogInfo(domain: string | undefined, name: string | undefined): CatalogInfo {
  const d = (domain ?? "").toLowerCase();
  const n = name ?? d.split(".")[0] ?? "";
  const url = d === "sky.de" ? null : deletionUrl(d);
  const di: DeletionInfo = {
    name: n, domains: [d], url, difficulty: d === "sky.de" ? "Impossible" : difficultyFor(d), notes: NOTES[d] ?? null,
    email: POST_ONLY.includes(d) ? null : `datenschutz@${d}`, legal_name: `${n} GmbH`, address: "Datenschutzbeauftragter\nMusterstraße 1\n10115 Berlin",
    privacy_email: POST_ONLY.includes(d) ? null : `datenschutz@${d}`, slug: d.split(".")[0] ?? null, relevant_countries: ["DE"],
  };
  const known = !!d && !d.startsWith("unresolved");
  return {
    deletion: known ? di : null,
    two_factor: SUPPORTS_2FA.includes(d) ? { methods: ["Authenticator App", "SMS"], documentation: `https://${d}/security/2fa` } : null,
    contact: known ? di : null,
  };
}

const ORGS: Organization[] = [
  { id: "schufa", name: "SCHUFA", legal_name: "SCHUFA Holding AG", kind: "CreditBureau", address_lines: ["SCHUFA Holding AG", "Privatkunden ServiceCenter", "Postfach 10 34 41", "50474 Köln"], email: null, web_form: "https://www.meineschufa.de/", description_de: "Größte Auskunftei in Deutschland. Die Datenkopie nach Art. 15 DSGVO ist kostenlos." },
  { id: "crif", name: "CRIF Bürgel", legal_name: "CRIF GmbH", kind: "CreditBureau", address_lines: ["CRIF GmbH", "Leopoldstraße 244", "80807 München"], email: "selbstauskunft@crif.com", web_form: null, description_de: "Auskunftei für Privatpersonen und Unternehmen." },
  { id: "infoscore", name: "Infoscore", legal_name: "infoscore Consumer Data GmbH", kind: "CreditBureau", address_lines: ["infoscore Consumer Data GmbH", "Rheinstraße 99", "76532 Baden-Baden"], email: null, web_form: "https://finance.arvato.com/", description_de: "Gehört zu Arvato Financial Solutions." },
  { id: "experian", name: "Experian", legal_name: "Experian GmbH", kind: "CreditBureau", address_lines: ["Experian GmbH", "Postfach 1010 14", "70010 Stuttgart"], email: null, web_form: null, description_de: "Nutzt Daten für Bonitätsauskünfte im Onlinehandel." },
  { id: "dp-direkt", name: "Deutsche Post Direkt", legal_name: "Deutsche Post Direkt GmbH", kind: "AddressBroker", address_lines: ["Deutsche Post Direkt GmbH", "Junkersring 57", "53844 Troisdorf"], email: "datenschutz@postdirekt.de", web_form: null, description_de: "Adresshändler. Widerspruch gegen Werbung und Weitergabe." },
  { id: "az-direct", name: "AZ Direct", legal_name: "AZ Direct GmbH", kind: "AddressBroker", address_lines: ["AZ Direct GmbH", "Carl Bertelsmann Straße 161 M", "33311 Gütersloh"], email: "datenschutz@az-direct.de", web_form: null, description_de: "Adresshändler der Arvato Gruppe." },
  { id: "robinson", name: "Robinsonliste", legal_name: "Deutscher Dialogmarketing Verband", kind: "OptOutList", address_lines: ["Deutscher Dialogmarketing Verband e. V.", "Hahnstraße 70", "60528 Frankfurt am Main"], email: null, web_form: "https://www.ddv.de/robinsonliste", description_de: "Sperrliste für Werbepost und Anrufe." },
];

const AUTH: Authority[] = [
  { state: "DE", name: "Bundesbeauftragte für den Datenschutz und die Informationsfreiheit", address_lines: ["Graurheindorfer Str. 153", "53117 Bonn"], email: "poststelle@bfdi.bund.de", complaint_form: "https://www.bfdi.bund.de/DE/Service/Kontakt/kontakt-node.html" },
  { state: "DE-BY", name: "Bayerisches Landesamt für Datenschutzaufsicht", address_lines: ["Promenade 18", "91522 Ansbach"], email: "poststelle@lda.bayern.de", complaint_form: "https://www.lda.bayern.de/de/beschwerde.html" },
  { state: "DE-BE", name: "Berliner Beauftragte für Datenschutz und Informationsfreiheit", address_lines: ["Alt-Moabit 59-61", "10555 Berlin"], email: "mailbox@datenschutz-berlin.de", complaint_form: "https://www.datenschutz-berlin.de/beschwerde" },
  { state: "DE-NW", name: "Landesbeauftragte für Datenschutz und Informationsfreiheit NRW", address_lines: ["Kavalleriestraße 2-4", "40213 Düsseldorf"], email: "poststelle@ldi.nrw.de", complaint_form: null },
];

export const REFERENCE: ReferenceData = {
  organizations: ORGS,
  authorities: AUTH,
  search_links: [
    { title_de: "Google: Ergebnisse zu mir entfernen", url: "https://support.google.com/websearch/troubleshooter/9685456", description_de: "Persönliche Daten aus der Suche entfernen lassen." },
    { title_de: "Have I Been Pwned", url: "https://haveibeenpwned.com/", description_de: "Prüft, ob deine Mailadresse in Datenlecks auftaucht." },
  ],
  payment_hubs: [
    { title_de: "PayPal Automatische Zahlungen", url: "https://www.paypal.com/myaccount/autopay/", description_de: "Alle Abos, die über PayPal laufen." },
    { title_de: "Apple Abos", url: "https://apps.apple.com/account/subscriptions", description_de: "Abos aus dem App Store und über Apple." },
    { title_de: "Google Play Abos", url: "https://play.google.com/store/account/subscriptions", description_de: "Abos über Google Play." },
    { title_de: "Amazon Zahlungen", url: "https://www.amazon.de/cpe/managepaymentmethods", description_de: "Gespeicherte Karten und Abos bei Amazon." },
  ],
  alias_services: [
    { title_de: "Apple E-Mail-Adresse verbergen", url: "https://support.apple.com/de-de/102567", description_de: "Zufällige Adresse, die an dein Postfach weiterleitet.", domains: ["privaterelay.appleid.com"] },
    { title_de: "SimpleLogin", url: "https://simplelogin.io/", description_de: "Eigene Aliasse mit Proton.", domains: ["simplelogin.com"] },
    { title_de: "Firefox Relay", url: "https://relay.firefox.com/", description_de: "Aliasse direkt im Browser.", domains: ["mozmail.com"] },
  ],
};

export const PRESETS: ProviderPreset[] = [
  { id: "gmail", name: "Gmail", imap_host: "imap.gmail.com", imap_port: 993, domains: ["gmail.com", "googlemail.com"], default_folders: ["INBOX", "[Gmail]/Alle Nachrichten"], app_password_url: "https://myaccount.google.com/apppasswords", hint_de: "Zweistufige Anmeldung aktivieren und ein App Passwort erzeugen. Das normale Passwort funktioniert nicht.", smtp: { host: "smtp.gmail.com", port: 465, implicit_tls: true } },
  { id: "outlook", name: "Outlook", imap_host: "outlook.office365.com", imap_port: 993, domains: ["outlook.com", "outlook.de", "hotmail.com", "hotmail.de", "live.de"], default_folders: ["INBOX"], app_password_url: "https://account.live.com/proofs/AppPassword", hint_de: "Für IMAP ist ein App Passwort nötig, wenn 2FA aktiv ist.", smtp: null },
  { id: "webde", name: "WEB.DE", imap_host: "imap.web.de", imap_port: 993, domains: ["web.de"], default_folders: ["INBOX"], app_password_url: null, hint_de: "In den Einstellungen unter POP3 und IMAP den Zugriff für externe Programme erlauben.", smtp: { host: "smtp.web.de", port: 587, implicit_tls: false } },
  { id: "gmx", name: "GMX", imap_host: "imap.gmx.net", imap_port: 993, domains: ["gmx.de", "gmx.net", "gmx.at", "gmx.ch"], default_folders: ["INBOX"], app_password_url: null, hint_de: "In den Einstellungen unter POP3 und IMAP den Zugriff für externe Programme erlauben.", smtp: { host: "mail.gmx.net", port: 587, implicit_tls: false } },
  { id: "icloud", name: "iCloud Mail", imap_host: "imap.mail.me.com", imap_port: 993, domains: ["icloud.com", "me.com", "mac.com"], default_folders: ["INBOX"], app_password_url: "https://account.apple.com/account/manage", hint_de: "App spezifisches Passwort erzeugen. Als Benutzername dient der Teil vor dem @.", smtp: null },
  { id: "tonline", name: "T Online", imap_host: "secureimap.t-online.de", imap_port: 993, domains: ["t-online.de"], default_folders: ["INBOX"], app_password_url: "https://www.telekom.de/hilfe/festnetz-internet-tv/e-mail/e-mail-programme", hint_de: "Ein separates Passwort für Mailprogramme im Kundencenter anlegen.", smtp: null },
  { id: "yahoo", name: "Yahoo", imap_host: "imap.mail.yahoo.com", imap_port: 993, domains: ["yahoo.com", "yahoo.de"], default_folders: ["INBOX"], app_password_url: "https://login.yahoo.com/account/security", hint_de: "App Passwort unter Kontosicherheit erzeugen.", smtp: null },
  { id: "posteo", name: "Posteo", imap_host: "posteo.de", imap_port: 993, domains: ["posteo.de", "posteo.net"], default_folders: ["INBOX"], app_password_url: null, hint_de: "Es gilt das normale Passwort des Postfachs.", smtp: null },
  { id: "mailbox", name: "mailbox.org", imap_host: "imap.mailbox.org", imap_port: 993, domains: ["mailbox.org"], default_folders: ["INBOX"], app_password_url: null, hint_de: "Bei aktivierter 2FA ein App Passwort anlegen.", smtp: null },
];

// ---------------------------------------------------------------------------------------------
// Insights (mirror of ausgeloggt-core insights.rs, simplified)
// ---------------------------------------------------------------------------------------------

const WEIGHT: Record<RiskReason, number> = {
  PasswordPwned: 30, IdentifierInBreach: 25, PaymentDataStored: 20, PasswordReused: 20, ActiveSubscription: 15,
  ServiceBreached: 15, ForeignRecoveryAddress: 15, No2faButSupported: 12, InactiveForYears: 10, PhoneLogin: 10, SsoLinked: 5,
};
const isOpen = (a: Account) => !["Deleted", "NotAnAccount", "Ignored"].includes(a.status);

function billingFor(a: Account): BillingSummary | null {
  if (!a.payments.length) return null;
  const spec = PAY[a.domain];
  const charges = a.payments.filter((p) => (p.kind === "Charge" || p.kind === "Renewal") && p.date).sort((x, y) => (x.date! < y.date! ? 1 : -1));
  const last = charges[0];
  const cancelled = a.payments.find((p) => p.kind === "Cancelled");
  const interval = spec?.interval ?? null;
  const active = !!interval && !cancelled;
  const monthly = last?.amount_minor && interval ? Math.round((last.amount_minor * PER_YEAR[interval]) / 12) : null;
  const methods: PaymentMethod[] = [];
  for (const p of a.payments) if (p.method && !methods.some((m) => JSON.stringify(m) === JSON.stringify(p.method))) methods.push(p.method);
  const year = now() - 365 * DAY;
  const spent = charges.filter((c) => new Date(c.date!).getTime() > year).reduce((s, c) => s + (c.amount_minor ?? 0), 0);
  return {
    account_id: a.id, active_subscription: active, interval, amount_minor: last?.amount_minor ?? null, currency: last?.currency ?? null,
    monthly_minor: monthly, plan: last?.plan ?? null, last_charge: last?.date ?? null,
    next_charge_estimate: active && last?.date ? new Date(new Date(last.date).getTime() + CYCLE[interval!] * DAY).toISOString() : null,
    price_increased: !!a.payments.find((p) => p.kind === "PriceChange"), cancelled_at: cancelled?.date ?? null, methods,
    via: last?.via ?? null, possibly_unused: active && !!spec?.unused, spent_last_12m: spent ? [["EUR", spent]] : [],
  };
}

export function assess(a: Account, ownDomains: string[], billing: BillingSummary | null): RiskAssessment {
  const out: RiskAssessment = { account_id: a.id, score: 0, reasons: [], next_step_de: "Nichts zu tun" };
  if (!isOpen(a)) return out;
  const s = a.security;
  const r: RiskReason[] = [];
  if (a.payments.some((p) => p.kind !== "Cancelled" && p.kind !== "Refund")) r.push("PaymentDataStored");
  if (billing?.active_subscription) r.push("ActiveSubscription");
  if (s.breaches.some((b) => !b.confirmed_for_user)) r.push("ServiceBreached");
  if (s.breaches.some((b) => b.confirmed_for_user)) r.push("IdentifierInBreach");
  if (s.password_pwned) r.push("PasswordPwned");
  if ((s.password_reuse_count ?? 0) > 0) r.push("PasswordReused");
  const tfa = s.has_2fa === true;
  if (SUPPORTS_2FA.includes(a.domain) && !tfa) r.push("No2faButSupported");
  const idle = a.last_seen ? (now() - new Date(a.last_seen).getTime()) / DAY : 0;
  if (idle >= 730) r.push("InactiveForYears");
  if (s.phone_login || a.identifiers.some((i) => /^\+?[\d\s]{7,}$/.test(i))) r.push("PhoneLogin");
  const free = ["gmail.com", "web.de", "gmx.de", "icloud.com", "outlook.com", "privaterelay.appleid.com"];
  if (a.identifiers.some((i) => i.includes("@") && !ownDomains.includes(i.split("@")[1]!) && !free.includes(i.split("@")[1]!))) r.push("ForeignRecoveryAddress");
  if (a.sso.length) r.push("SsoLinked");
  r.sort((x, y) => WEIGHT[y] - WEIGHT[x]);
  out.reasons = r;
  out.score = Math.min(100, r.reduce((t, x) => t + WEIGHT[x], 0));
  const has = (x: RiskReason) => r.includes(x);
  out.next_step_de = a.status === "DeletionPending" ? "Löschung läuft, Antwort abwarten"
    : has("PasswordPwned") ? "Passwort ändern und 2FA aktivieren"
      : has("ActiveSubscription") ? "Erst Abo kündigen, dann Account löschen"
        : has("IdentifierInBreach") || has("PasswordReused") || has("ServiceBreached") ? (tfa ? "Passwort ändern" : "Passwort ändern und 2FA aktivieren")
          : has("ForeignRecoveryAddress") ? "Login Adresse auf eine eigene Adresse umstellen"
            : has("InactiveForYears") ? `Account löschen, seit ${Math.floor(idle / 365)} Jahren ungenutzt`
              : has("No2faButSupported") ? "2FA aktivieren"
                : has("PhoneLogin") ? "Anmeldung auf Mailadresse und Authenticator umstellen"
                  : has("PaymentDataStored") ? "Zahlungsdaten im Konto entfernen"
                    : has("SsoLinked") ? "Verknüpfung mit dem Drittanbieter prüfen" : "Nichts zu tun";
  return out;
}

export function computeInsights(snap: Snapshot): InsightsDto {
  const own = snap.mailboxes.map((m) => m.address.split("@")[1]!).filter(Boolean);
  const billing = snap.accounts.map(billingFor).filter((b): b is BillingSummary => !!b);
  const bmap = new Map(billing.map((b) => [b.account_id, b]));
  const risks = snap.accounts.filter(isOpen).map((a) => assess(a, own, bmap.get(a.id) ?? null))
    .sort((x, y) => y.score - x.score);

  const tot = new Map<string, [number, number]>();
  for (const b of billing) {
    if (!b.active_subscription || !b.monthly_minor || !b.currency) continue;
    const t = tot.get(b.currency) ?? [0, 0];
    tot.set(b.currency, [t[0] + b.monthly_minor, t[1] + 1]);
  }
  const totals: InsightsDto["totals"] = [...tot.entries()].map(([c, [m, n]]) => [c, m, m * 12, n]);

  const idx = new Map<string, [PaymentMethod, string[]]>();
  const payAcc: string[] = [];
  for (const a of snap.accounts) {
    if (!isOpen(a) || !a.payments.length) continue;
    payAcc.push(a.id);
    for (const p of a.payments) {
      if (!p.method || p.kind === "Cancelled" || p.kind === "Refund") continue;
      const k = JSON.stringify(p.method);
      const e = idx.get(k) ?? [p.method, []];
      if (!e[1].includes(a.id)) e[1].push(a.id);
      idx.set(k, e);
    }
  }
  const payment_index = [...idx.values()].sort((x, y) => y[1].length - x[1].length);

  const overdue = snap.requests.filter((r) => r.status === "Sent" && r.deadline && new Date(r.deadline).getTime() < now()).map((r) => r.id);
  const open = snap.accounts.filter(isOpen);
  const relevant = snap.accounts.filter((a) => !["NotAnAccount", "Ignored"].includes(a.status));
  const reviewed = relevant.filter((a) => a.status !== "Discovered").length;
  const capable = open.filter((a) => SUPPORTS_2FA.includes(a.domain));
  const with2 = capable.filter((a) => a.security.has_2fa === true).length;
  const audited = open.filter((a) => a.security.password_reuse_count !== null || a.security.password_pwned !== null);
  const clean = audited.filter((a) => (a.security.password_reuse_count ?? 0) === 0 && !a.security.password_pwned).length;
  const high = risks.filter((r) => r.score >= 60).length;
  const mkt = snap.subscriptions.filter((s) => s.category === "Marketing" || s.category === "Spam");
  const handled = mkt.filter((s) => s.status === "Unsubscribed" || s.status === "Keep").length;
  const sent = snap.requests.filter((r) => r.status !== "Draft");
  const answered = sent.filter((r) => ["Answered", "Done", "Rejected"].includes(r.status)).length;
  const sh = (g: number, t: number) => (t === 0 ? 1 : g / t);
  const pts = (s: number, m: number) => Math.min(m, Math.round(Math.max(0, Math.min(1, s)) * m));
  const parts: [string, number, number][] = [
    ["Accounts aufgeräumt", pts(sh(reviewed, relevant.length), 25), 25],
    ["2FA aktiv", pts(sh(with2, capable.length), 15), 15],
    ["Einzigartige Passwörter", pts(sh(clean, audited.length), 15), 15],
    ["Wenige Risikoaccounts", pts(1 - (open.length ? high / open.length : 0), 15), 15],
    ["Werbung abgemeldet", pts(sh(handled, mkt.length), 10), 10],
    ["Anfragen beantwortet", pts(sh(answered, sent.length), 10), 10],
    ["Fristen eingehalten", Math.max(0, 10 - 5 * overdue.length), 10],
  ];
  const score: PrivacyScore = {
    has_data: snap.accounts.length > 0, score: Math.min(100, parts.reduce((t, p) => t + p[1], 0)), parts, open_accounts: open.length,
    deleted_accounts: snap.accounts.filter((a) => a.status === "Deleted").length, high_risk_accounts: high,
    pending_requests: sent.filter((r) => r.status === "Sent").length, overdue_requests: overdue.length,
  };

  const mailboxes = new Set(snap.mailboxes.map((m) => m.address.toLowerCase()));
  const aliases: AliasAdvice[] = snap.accounts
    .filter((a) => a.status === "Keep" || a.status === "Confirmed")
    .filter((a) => !a.identifiers.some((i) => i.endsWith("privaterelay.appleid.com")))
    .flatMap((a) => {
      const id = a.identifiers.find((i) => mailboxes.has(i.toLowerCase()));
      return id ? [{ account_id: a.id, current_identifier: id, reason_de: "Der Dienst kennt deine Hauptadresse. Mit einem Alias lässt sich der Dienst später leicht abklemmen." }] : [];
    });

  return { billing, totals, payment_index, payment_accounts: payAcc, risks, score, aliases, overdue_requests: overdue };
}

// ---------------------------------------------------------------------------------------------
// Letters
// ---------------------------------------------------------------------------------------------

const today = () => new Date().toLocaleDateString("de-DE", { day: "2-digit", month: "long", year: "numeric" });

export function makeLetter(snap: Snapshot, a: LetterArgs): LetterPreview {
  const id = snap.settings.identity;
  const missing: string[] = [];
  if (!snap.settings.full_name.trim()) missing.push("Name");
  if (!id.street.trim()) missing.push("Straße");
  if (!id.postal_code.trim()) missing.push("PLZ");
  if (!id.city.trim()) missing.push("Ort");
  const name = snap.settings.full_name.trim() || "[Name]";
  const sender = `${name}\n${id.street.trim() || "[Straße]"}\n${id.postal_code.trim() || "[PLZ]"} ${id.city.trim() || "[Ort]"}`;

  const account = a.accountId ? snap.accounts.find((x) => x.id === a.accountId) : undefined;
  const sub = a.subscriptionId ? snap.subscriptions.find((x) => x.id === a.subscriptionId) : undefined;
  const req = a.requestId ? snap.requests.find((x) => x.id === a.requestId) : undefined;
  const org = a.orgId ? REFERENCE.organizations.find((x) => x.id === a.orgId) : undefined;

  let recipient: Party | null = a.recipient ?? null;
  let target = account?.name ?? sub?.name ?? req?.target ?? org?.name ?? "";
  if (!recipient) {
    if (a.kind === "Complaint") {
      const st = a.authorityState || id.state || "DE";
      const au = REFERENCE.authorities.find((x) => x.state === st) ?? REFERENCE.authorities[0]!;
      recipient = { name: au.name, address_lines: au.address_lines, email: au.email };
      target = au.name;
    } else if (org) recipient = { name: org.legal_name, address_lines: org.address_lines, email: org.email };
    else if (account) {
      const c = catalogInfo(account.domain, account.name).contact;
      recipient = c ? { name: c.legal_name ?? account.name, address_lines: (c.address ?? "").split("\n"), email: c.privacy_email ?? c.email } : null;
    } else if (sub) recipient = { name: sub.name, address_lines: [], email: `datenschutz@${sub.domain}` };
    else if (req) recipient = { name: req.target, address_lines: req.recipient.split("\n"), email: req.recipient.includes("@") ? req.recipient : null };
  }
  const rec = recipient ?? { name: "[Empfänger]", address_lines: [], email: null };
  const idents = account?.identifiers.join(", ") ?? "";
  const head = `${sender}\n\n${rec.name}\n${rec.address_lines.join("\n")}\n\n${today()}\n\n`;
  const foot = `\n\nMit freundlichen Grüßen\n${name}`;
  let subject = "";
  let text = "";
  let deadline: string | null = inDays(30);
  switch (a.kind) {
    case "Erasure":
      subject = `Antrag auf Löschung (Art. 17 DSGVO), ${target}`;
      text = `Sehr geehrte Damen und Herren,\n\nhiermit fordere ich Sie gemäß Art. 17 DSGVO auf, alle zu mir gespeicherten personenbezogenen Daten unverzüglich zu löschen, insbesondere mein Kundenkonto bei ${target}.\n\nKennungen des Kontos: ${idents}\n\nBitte bestätigen Sie mir die vollständige Löschung innerhalb eines Monats schriftlich (Art. 12 Abs. 3 DSGVO).`;
      break;
    case "Access":
      subject = `Auskunft nach Art. 15 DSGVO, ${target}`;
      text = `Sehr geehrte Damen und Herren,\n\nich bitte Sie gemäß Art. 15 DSGVO um Auskunft über die zu mir gespeicherten personenbezogenen Daten, deren Zwecke, Empfänger und Speicherdauer sowie um eine Kopie der Daten.\n\nKennungen des Kontos: ${idents}\n\nBitte antworten Sie innerhalb eines Monats.`;
      break;
    case "ObjectionAndAccess":
      subject = `Widerspruch und Auskunft (Art. 21, 15 DSGVO), ${target}`;
      text = `Sehr geehrte Damen und Herren,\n\nich widerspreche der Verarbeitung meiner Daten für Werbezwecke (Art. 21 Abs. 2 DSGVO) und bitte um Auskunft nach Art. 15 DSGVO, woher Sie meine Daten haben.\n\nKennungen des Kontos: ${idents}`;
      break;
    case "Cancellation": {
      deadline = inDays(14);
      subject = `Kündigung meines Vertrags bei ${target}`;
      text = `Sehr geehrte Damen und Herren,\n\nhiermit kündige ich meinen Vertrag bei ${target} fristgerecht zum nächstmöglichen Zeitpunkt. Bitte bestätigen Sie mir die Kündigung und das Vertragsende schriftlich.\n\nKennungen des Kontos: ${idents}`;
      break;
    }
    case "CeaseAndDesist":
      deadline = inDays(14);
      subject = `Unterlassung von Werbemails, ${target}`;
      text = `Sehr geehrte Damen und Herren,\n\nich habe mich von Ihren Werbemails abgemeldet, erhalte aber weiterhin Mails (zuletzt am ${sub?.post_unsubscribe_mails[0]?.date ? new Date(sub.post_unsubscribe_mails[0].date).toLocaleDateString("de-DE") : "[Datum]"}). Ich fordere Sie auf, den Versand an ${sub?.mailbox ?? "[Adresse]"} binnen 14 Tagen zu unterlassen (§ 7 UWG, Art. 21 DSGVO).`;
      break;
    case "CreditBureauAccess":
      subject = "Datenkopie nach Art. 15 DSGVO";
      text = `Sehr geehrte Damen und Herren,\n\nich bitte gemäß Art. 15 DSGVO um eine kostenlose Datenkopie zu allen über mich gespeicherten Daten, einschließlich Scorewerten und deren Berechnung. Ich füge eine Ausweiskopie mit geschwärzten Angaben bei.\n\nGeburtsdatum: ${id.birth_date || "[Geburtsdatum]"}`;
      break;
    case "AddressBrokerObjection":
      subject = "Widerspruch gegen Werbung und Datenweitergabe";
      text = `Sehr geehrte Damen und Herren,\n\nich widerspreche der Nutzung und Weitergabe meiner Daten für Werbezwecke (Art. 21 Abs. 2 DSGVO) und bitte um Löschung. Zudem bitte ich um Auskunft nach Art. 15 DSGVO, woher Sie meine Daten haben.`;
      break;
    case "Complaint":
      deadline = null;
      subject = `Beschwerde nach Art. 77 DSGVO gegen ${req?.target ?? "[Verantwortlicher]"}`;
      text = `Sehr geehrte Damen und Herren,\n\nich beschwere mich über ${req?.target ?? "[Verantwortlicher]"}. Mein Antrag vom ${req?.sent_at ? new Date(req.sent_at).toLocaleDateString("de-DE") : "[Datum]"} wurde innerhalb der Frist von einem Monat nicht beantwortet. Ich bitte Sie, tätig zu werden.`;
      break;
    case "Reminder":
      deadline = inDays(14);
      subject = `Erinnerung: ${req?.subject ?? "mein Antrag"}`;
      text = `Sehr geehrte Damen und Herren,\n\nich erinnere an meinen Antrag vom ${req?.sent_at ? new Date(req.sent_at).toLocaleDateString("de-DE") : "[Datum]"}. Die Frist ist abgelaufen. Bitte antworten Sie innerhalb von 14 Tagen.`;
      break;
  }
  return {
    subject, text: head + text + foot, recipient: rec, recipient_email: rec.email ?? null, deadline,
    missing_sender_fields: missing, missing_recipient: !recipient || (!rec.email && rec.address_lines.length === 0),
  };
}
