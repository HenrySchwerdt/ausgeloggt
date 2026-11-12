// Realistic FAKE demo data (German user "Lena Hartmann"). Nothing here is real.
import type {
  Account, AccountStatus, DeletionStep, Evidence, EvidenceKind, MailCategory, SourceKind, SourceRun,
  SsoProvider, Subscription, SubscriptionStatus, UnsubscribeMethod,
} from "../types";

const DAY = 86_400_000;
const now = Date.now();
const ago = (d: number, h = 0) => new Date(now - d * DAY - h * 3_600_000).toISOString();
const inDays = (d: number) => new Date(now + d * DAY).toISOString();

let seed = 7;
const rnd = () => ((seed = (seed * 1664525 + 1013904223) % 4294967296) / 4294967296);
const uuid = (n: number, p = "a") =>
  `${p}${n.toString(16).padStart(7, "0")}-0000-4000-8000-${(n * 7919).toString(16).padStart(12, "0")}`;

export const MAILBOXES = ["lena.hartmann@gmail.com", "l.hartmann@web.de"];
const G = MAILBOXES[0]!;
const W = MAILBOXES[1]!;

const mail = (mailbox: string): SourceKind => ({ Mail: { mailbox } });
const pw = (format: "Chrome" | "Bitwarden" | "Safari" | "OnePassword"): SourceKind => ({ PasswordExport: { format } });

type Row = {
  n: string; d: string; s: AccountStatus; src: SourceKind[]; id: string; conf: number; days: number;
  sso?: SsoProvider[]; k?: EvidenceKind[]; note?: string; tags?: string[];
};

const apple = (code: string) => `${code}@privaterelay.appleid.com`;

const R: Row[] = [
  { n: "Spotify", d: "spotify.com", s: "Confirmed", src: [mail(G), pw("Chrome")], id: G, conf: 0.97, days: 2310, k: ["Welcome", "SavedPassword"] },
  { n: "Netflix", d: "netflix.com", s: "Keep", src: [mail(G)], id: G, conf: 0.94, days: 2890, k: ["Welcome", "LoginAlert"] },
  { n: "Amazon", d: "amazon.de", s: "Keep", src: [mail(G), pw("Chrome")], id: G, conf: 0.99, days: 3900, k: ["Receipt", "SavedPassword"] },
  { n: "PayPal", d: "paypal.com", s: "Keep", src: [mail(G), pw("Chrome")], id: G, conf: 0.98, days: 3400, k: ["LoginAlert", "SavedPassword"] },
  { n: "GitHub", d: "github.com", s: "Keep", src: [pw("Bitwarden"), "GitHub"], id: "lhartmann", conf: 0.99, days: 2700, k: ["SavedPassword", "Manual"] },
  { n: "Discord", d: "discord.com", s: "Confirmed", src: [mail(G)], id: G, conf: 0.93, days: 1800, sso: ["Google"], k: ["Verification", "SsoGrant"] },
  { n: "Adobe", d: "adobe.com", s: "DeletionPending", src: [mail(W), pw("Bitwarden")], id: W, conf: 0.96, days: 2500, k: ["Welcome", "Receipt"], note: "Abo bereits gekündigt, Daten sollen gelöscht werden." },
  { n: "Dropbox", d: "dropbox.com", s: "Confirmed", src: [mail(G), pw("Chrome")], id: G, conf: 0.95, days: 2900, sso: ["Google"], k: ["Welcome", "LoginAlert"] },
  { n: "Zalando", d: "zalando.de", s: "DeletionPending", src: [mail(W), pw("Chrome")], id: W, conf: 0.97, days: 2100, k: ["Welcome", "Receipt", "Newsletter"] },
  { n: "Lieferando", d: "lieferando.de", s: "Discovered", src: [mail(G)], id: G, conf: 0.81, days: 900, k: ["Receipt", "Newsletter"] },
  { n: "Deutsche Bahn", d: "bahn.de", s: "Keep", src: [mail(W), pw("Chrome")], id: W, conf: 0.97, days: 3100, k: ["Receipt", "SavedPassword"] },
  { n: "Airbnb", d: "airbnb.de", s: "Discovered", src: [mail(G)], id: G, conf: 0.88, days: 1500, sso: ["Facebook"], k: ["Welcome", "SsoGrant"] },
  { n: "Uber", d: "uber.com", s: "Discovered", src: [mail(G)], id: G, conf: 0.9, days: 1300, sso: ["Apple"], k: ["Receipt", "Welcome"] },
  { n: "Notion", d: "notion.so", s: "Confirmed", src: [mail(G), pw("Bitwarden")], id: G, conf: 0.96, days: 1100, sso: ["Google"], k: ["Welcome", "LoginAlert"] },
  { n: "Figma", d: "figma.com", s: "Discovered", src: [mail(G)], id: G, conf: 0.84, days: 800, sso: ["Google"], k: ["Verification"] },
  { n: "Slack", d: "slack.com", s: "Discovered", src: [mail(G), pw("Chrome")], id: G, conf: 0.9, days: 1250, k: ["Welcome", "SavedPassword"] },
  { n: "Steam", d: "steampowered.com", s: "Keep", src: [pw("Bitwarden"), mail(G)], id: "lena_h89", conf: 0.99, days: 4100, k: ["SavedPassword", "Receipt"] },
  { n: "Epic Games", d: "epicgames.com", s: "Discovered", src: [mail(G)], id: G, conf: 0.89, days: 1400, sso: ["Discord"], k: ["Verification", "SsoGrant"] },
  { n: "Twitch", d: "twitch.tv", s: "Discovered", src: [mail(G)], id: "lenah_89", conf: 0.78, days: 1900, sso: ["Amazon"], k: ["Welcome", "SsoGrant"] },
  { n: "Reddit", d: "reddit.com", s: "Confirmed", src: [mail(G), pw("Chrome")], id: "lena_h89", conf: 0.92, days: 2800, sso: ["Google"], k: ["Welcome", "SavedPassword"] },
  { n: "LinkedIn", d: "linkedin.com", s: "Keep", src: [mail(W), pw("Chrome")], id: W, conf: 0.98, days: 3300, k: ["Welcome", "LoginAlert"] },
  { n: "X", d: "x.com", s: "DeletionPending", src: [mail(G)], id: "@lena_h89", conf: 0.91, days: 3000, k: ["Welcome", "PasswordReset"] },
  { n: "Instagram", d: "instagram.com", s: "Discovered", src: [mail(G), "FacebookExport"], id: "lena.hartmann", conf: 0.95, days: 2400, sso: ["Facebook"], k: ["LoginAlert", "DataExport"] },
  { n: "TikTok", d: "tiktok.com", s: "Discovered", src: [mail(G)], id: apple("xk3p9d7vq2"), conf: 0.86, days: 700, sso: ["Apple"], k: ["Verification", "SsoGrant"] },
  { n: "Duolingo", d: "duolingo.com", s: "Discovered", src: [mail(G)], id: apple("m7q2zt5h1b"), conf: 0.83, days: 500, sso: ["Apple"], k: ["Welcome", "Newsletter"] },
  { n: "eBay", d: "ebay.de", s: "Confirmed", src: [mail(W), pw("Chrome")], id: W, conf: 0.97, days: 3800, k: ["Receipt", "SavedPassword"] },
  { n: "Otto", d: "otto.de", s: "Deleted", src: [mail(W)], id: W, conf: 0.94, days: 3200, k: ["Receipt", "Welcome"] },
  { n: "IKEA", d: "ikea.com", s: "Discovered", src: [mail(G)], id: G, conf: 0.79, days: 600, k: ["Receipt", "Newsletter"] },
  { n: "Booking.com", d: "booking.com", s: "DeletionPending", src: [mail(G)], id: G, conf: 0.95, days: 1700, sso: ["Google"], k: ["Welcome", "Receipt"] },
  { n: "Pinterest", d: "pinterest.de", s: "Discovered", src: [mail(G)], id: apple("c8n4w1r6ya"), conf: 0.74, days: 1000, sso: ["Apple"], k: ["Welcome"] },
  { n: "Canva", d: "canva.com", s: "Discovered", src: [mail(G)], id: apple("t2h9k5v0dp"), conf: 0.82, days: 450, sso: ["Apple"], k: ["Verification"] },
  { n: "Zoom", d: "zoom.us", s: "Confirmed", src: [mail(W), pw("Chrome")], id: W, conf: 0.93, days: 1400, sso: ["Google"], k: ["Welcome", "LoginAlert"] },
  { n: "Strava", d: "strava.com", s: "Discovered", src: [mail(G)], id: apple("j5b3x8nq7e"), conf: 0.85, days: 380, sso: ["Apple"], k: ["Welcome", "LoginAlert"] },
  { n: "Medium", d: "medium.com", s: "Discovered", src: [mail(G)], id: apple("p1d6s4yf9m"), conf: 0.62, days: 800, sso: ["Apple"], k: ["Newsletter", "Welcome"] },
  { n: "Trello", d: "trello.com", s: "Discovered", src: [mail(G)], id: G, conf: 0.72, days: 1500, sso: ["Google"], k: ["Verification"] },
  { n: "Asana", d: "asana.com", s: "Ignored", src: [mail(W)], id: W, conf: 0.55, days: 1100, k: ["Welcome"] },
  { n: "Vinted", d: "vinted.de", s: "Discovered", src: [mail(G), pw("Safari")], id: G, conf: 0.9, days: 540, k: ["Welcome", "SavedPassword"] },
  { n: "Etsy", d: "etsy.com", s: "Confirmed", src: [mail(G)], id: G, conf: 0.87, days: 1250, k: ["Receipt", "Welcome"] },
  { n: "Klarna", d: "klarna.com", s: "Keep", src: [mail(G), pw("Chrome")], id: G, conf: 0.96, days: 1600, k: ["LoginAlert", "SavedPassword"] },
  { n: "Revolut", d: "revolut.com", s: "Keep", src: [mail(G), pw("Bitwarden")], id: G, conf: 0.99, days: 1700, k: ["LoginAlert", "SavedPassword"] },
  { n: "N26", d: "n26.com", s: "Deleted", src: [mail(G)], id: G, conf: 0.97, days: 2500, k: ["Welcome", "LoginAlert"] },
  { n: "Flixbus", d: "flixbus.de", s: "Discovered", src: [mail(W)], id: W, conf: 0.76, days: 700, k: ["Receipt", "Newsletter"] },
  { n: "Deezer", d: "deezer.com", s: "Discovered", src: [mail(W)], id: W, conf: 0.69, days: 2300, k: ["Welcome", "Newsletter"] },
  { n: "SoundCloud", d: "soundcloud.com", s: "Discovered", src: [mail(G)], id: "lenah", conf: 0.66, days: 2600, sso: ["Facebook"], k: ["Welcome", "SsoGrant"] },
  { n: "Vimeo", d: "vimeo.com", s: "Discovered", src: [mail(G)], id: G, conf: 0.58, days: 2000, k: ["Verification"] },
  { n: "Tumblr", d: "tumblr.com", s: "Discovered", src: [mail(G)], id: "hartmann-lena", conf: 0.52, days: 3700, k: ["Welcome", "PasswordReset"] },
  { n: "Quora", d: "quora.com", s: "NotAnAccount", src: [mail(G)], id: G, conf: 0.31, days: 2200, k: ["Newsletter"] },
  { n: "Coursera", d: "coursera.org", s: "Discovered", src: [mail(G)], id: G, conf: 0.8, days: 1300, sso: ["Google"], k: ["Welcome", "Receipt"] },
  { n: "Udemy", d: "udemy.com", s: "Discovered", src: [mail(G), pw("Chrome")], id: G, conf: 0.88, days: 1450, k: ["Receipt", "SavedPassword"] },
  { n: "Patreon", d: "patreon.com", s: "Discovered", src: [mail(G)], id: apple("g4v7c2ld0s"), conf: 0.77, days: 420, sso: ["Apple"], k: ["Receipt", "SsoGrant"] },
  { n: "Shopify", d: "shopify.com", s: "Discovered", src: [mail(W)], id: W, conf: 0.57, days: 900, k: ["Verification"] },
  { n: "Stripe", d: "stripe.com", s: "Discovered", src: [pw("Bitwarden")], id: W, conf: 0.9, days: 1200, k: ["SavedPassword"] },
  { n: "Mailchimp", d: "mailchimp.com", s: "Discovered", src: [mail(W)], id: W, conf: 0.6, days: 1600, k: ["Welcome", "Newsletter"] },
  { n: "Wise", d: "wise.com", s: "Keep", src: [mail(G), pw("Bitwarden")], id: G, conf: 0.98, days: 1000, k: ["LoginAlert", "SavedPassword"] },
  { n: "Lidl Plus", d: "lidl.de", s: "Discovered", src: [mail(G)], id: apple("w9f2h5jq3k"), conf: 0.7, days: 300, sso: ["Apple"], k: ["Welcome"] },
  { n: "MediaMarkt", d: "mediamarkt.de", s: "Discovered", src: [mail(W)], id: W, conf: 0.73, days: 1700, k: ["Receipt", "Newsletter"] },
  { n: "HelloFresh", d: "hellofresh.de", s: "DeletionPending", src: [mail(G)], id: G, conf: 0.92, days: 1100, k: ["Welcome", "Receipt"] },
  { n: "Sky", d: "sky.de", s: "Discovered", src: [mail(W)], id: W, conf: 0.64, days: 2900, k: ["Receipt", "Newsletter"] },
  { n: "Disney+", d: "disneyplus.com", s: "Discovered", src: [mail(G)], id: apple("r3k8m1ez5t"), conf: 0.8, days: 600, sso: ["Apple"], k: ["Welcome", "SsoGrant"] },
  { n: "Audible", d: "audible.de", s: "Discovered", src: [mail(G)], id: G, conf: 0.86, days: 1500, sso: ["Amazon"], k: ["Welcome", "Receipt"] },
  { n: "Letterboxd", d: "letterboxd.com", s: "Discovered", src: [mail(G)], id: apple("y6u1b9xo4n"), conf: 0.71, days: 280, sso: ["Apple"], k: ["Welcome"] },
  { n: "Bandcamp", d: "bandcamp.com", s: "Discovered", src: [mail(G)], id: G, conf: 0.83, days: 2100, k: ["Receipt", "Welcome"] },
  { n: "Goodreads", d: "goodreads.com", s: "Discovered", src: [mail(G)], id: G, conf: 0.67, days: 3600, sso: ["Amazon"], k: ["Welcome"] },
  { n: "Miro", d: "miro.com", s: "Discovered", src: [mail(W)], id: W, conf: 0.75, days: 700, sso: ["Google"], k: ["Verification", "SsoGrant"] },
  { n: "Linktree", d: "linktr.ee", s: "Discovered", src: [mail(G)], id: apple("h2z5n8qa7c"), conf: 0.68, days: 350, sso: ["Apple"], k: ["Welcome"] },
  { n: "Threads", d: "threads.net", s: "Discovered", src: ["FacebookExport"], id: "lena.hartmann", conf: 0.8, days: 400, sso: ["Facebook"], k: ["DataExport"] },
  { n: "Snapchat", d: "snapchat.com", s: "Discovered", src: [mail(G)], id: "lena_h89", conf: 0.59, days: 2900, k: ["Verification", "LoginAlert"] },
  { n: "WhatsApp", d: "whatsapp.com", s: "Keep", src: ["GoogleTakeout"], id: "+49 171 5550184", conf: 0.99, days: 3500, k: ["DataExport"] },
  { n: "Google", d: "google.com", s: "Keep", src: ["GoogleTakeout", pw("Chrome")], id: G, conf: 1, days: 5000, k: ["DataExport", "SavedPassword"] },
  { n: "Apple", d: "apple.com", s: "Keep", src: [mail(G), pw("Safari")], id: G, conf: 1, days: 4800, k: ["Receipt", "SavedPassword"] },
  { n: "Microsoft", d: "microsoft.com", s: "Confirmed", src: [mail(W), pw("Chrome")], id: W, conf: 0.98, days: 4200, k: ["LoginAlert", "SavedPassword"] },
  { n: "Skyscanner", d: "skyscanner.de", s: "Discovered", src: [mail(G)], id: apple("d7s0p3tk8w"), conf: 0.55, days: 650, sso: ["Apple"], k: ["Newsletter", "Welcome"] },
  { n: "Tinder", d: "tinder.com", s: "Discovered", src: ["FacebookExport"], id: "lena.hartmann", conf: 0.73, days: 2700, sso: ["Facebook"], k: ["DataExport"] },
  { n: "Ryanair", d: "ryanair.com", s: "Discovered", src: [mail(G)], id: G, conf: 0.84, days: 1100, k: ["Receipt", "Verification"] },
  { n: "Wix", d: "wix.com", s: "Deleted", src: [mail(W)], id: W, conf: 0.9, days: 3000, k: ["Welcome", "Receipt"] },
];

const KIND_SUMMARY: Record<EvidenceKind, (n: string) => string> = {
  Welcome: (n) => `Willkommen bei ${n}`,
  Verification: (n) => `Bitte bestätige deine E-Mail-Adresse (${n})`,
  PasswordReset: (n) => `Passwort zurücksetzen – ${n}`,
  LoginAlert: (n) => `Neue Anmeldung bei deinem ${n}-Konto`,
  SsoGrant: (n) => `${n} wurde Zugriff auf dein Konto gewährt`,
  SavedPassword: (n) => `Gespeicherter Login für ${n}`,
  DataExport: (n) => `${n} im Datenexport gefunden`,
  Breach: (n) => `${n} in bekanntem Datenleck`,
  Receipt: (n) => `Deine Bestellung bei ${n} – Rechnung`,
  Newsletter: (n) => `${n} Newsletter: Angebote der Woche`,
  OtherMail: (n) => `Hinweis von ${n}`,
  Manual: (n) => `${n} manuell hinzugefügt`,
};

function sourceFor(kind: EvidenceKind, src: SourceKind[]): SourceKind {
  const find = (f: (s: SourceKind) => boolean) => src.find(f);
  if (kind === "SavedPassword") return find((s) => typeof s !== "string" && "PasswordExport" in s) ?? src[0]!;
  if (kind === "DataExport") return find((s) => typeof s === "string") ?? src[0]!;
  if (kind === "Manual") return "Manual";
  return find((s) => typeof s !== "string" && "Mail" in s) ?? src[0]!;
}

export interface CatalogInfo {
  difficulty: "easy" | "medium" | "hard" | "impossible" | "unknown";
  deletion_url: string | null;
  contact_email: string | null;
  note: string | null;
}

const HARD = new Set(["amazon.de", "adobe.com", "netflix.com", "linkedin.com", "x.com", "airbnb.de", "booking.com", "tumblr.com", "snapchat.com", "sky.de"]);
const EASY = new Set(["spotify.com", "github.com", "discord.com", "zalando.de", "reddit.com", "notion.so", "duolingo.com", "figma.com", "twitch.tv", "canva.com", "strava.com", "medium.com", "etsy.com", "vinted.de"]);
const NOTES: Record<string, string> = {
  "amazon.de": "Konto lässt sich nur über den Kundenservice schließen. Offene Bestellungen vorher abwarten.",
  "adobe.com": "Löschung nur nach Kündigung aller Abos. Datenlöschung separat per Datenschutz-Anfrage.",
  "netflix.com": "Nur Kündigung online möglich, Löschung der Daten per Anfrage an den Datenschutz.",
  "x.com": "Konto wird 30 Tage deaktiviert und danach endgültig gelöscht.",
  "linkedin.com": "Konto schließen unter Einstellungen > Kontoverwaltung. Löschung nach 24 Stunden.",
};

export function catalogFor(domain: string): CatalogInfo {
  const difficulty = HARD.has(domain) ? "hard" : EASY.has(domain) ? "easy" : "medium";
  return {
    difficulty,
    deletion_url: domain === "tumblr.com" ? null : `https://www.${domain}/account/delete`,
    contact_email: `datenschutz@${domain}`,
    note: NOTES[domain] ?? null,
  };
}

/** [days since the deletion started, GDPR letter sent?] */
const PENDING: Record<string, [number, boolean]> = {
  "adobe.com": [34, true], "zalando.de": [12, false], "x.com": [41, true], "booking.com": [5, false], "hellofresh.de": [21, false],
};

export function buildAccounts(): Account[] {
  const out: Account[] = [];
  R.forEach((r, i) => {
    const kinds = r.k ?? ["Welcome"];
    const first = ago(r.days);
    const last = ago(Math.floor(rnd() * Math.min(r.days, 120)) + 1);
    const evidence: Evidence[] = kinds.map((k, j) => ({
      kind: k,
      source: sourceFor(k, r.src),
      summary: KIND_SUMMARY[k](r.n),
      date: ago(Math.max(1, Math.floor(r.days / (j + 1.3) - rnd() * 40))),
      source_ref: `ref-${i}-${j}`,
    }));
    if (r.sso?.length && !kinds.includes("SsoGrant")) {
      evidence.push({ kind: "SsoGrant", source: r.src[0]!, summary: KIND_SUMMARY.SsoGrant(r.n), date: ago(Math.floor(r.days / 2)), source_ref: `ref-${i}-s` });
    }
    const identifiers = [r.id];
    if (i % 9 === 0 && r.id !== W) identifiers.push(W);
    const deletion: DeletionStep[] = [];
    const cat = catalogFor(r.d);
    if (r.s === "DeletionPending") {
      const [d0, gdpr] = PENDING[r.d] ?? [7, false];
      deletion.push({ at: ago(d0), method: { DirectLink: { url: cat.deletion_url ?? `https://${r.d}` } }, note: "Formular ausgefüllt, Bestätigung steht aus.", deadline: gdpr ? null : inDays(30 - d0) });
      if (gdpr) deletion.push({ at: ago(d0 - 1), method: { GdprEmail: { to: cat.contact_email! } }, note: "DSGVO-Antrag nach Art. 17 versendet.", deadline: inDays(30 - d0 + 1) });
    }
    if (r.s === "Deleted") {
      deletion.push({ at: ago(60), method: { GdprEmail: { to: cat.contact_email! } }, note: "DSGVO-Antrag versendet.", deadline: ago(30) });
      deletion.push({ at: ago(41), method: "Confirmed", note: "Bestätigung per E-Mail erhalten.", deadline: null });
    }
    out.push({
      id: uuid(i + 1), domain: r.d, name: r.n, identifiers, sso: r.sso ?? [], evidence, status: r.s,
      confidence: r.conf, first_seen: first, last_seen: last, tags: r.tags ?? [], notes: r.note ?? "",
      deletion, created_at: first, updated_at: last,
    });
  });
  return out;
}

type SubRow = [string, string, MailCategory, SubscriptionStatus, number, number, ("one" | "mail" | "link")[], string[]];
const SUBS: SubRow[] = [
  ["Zalando Newsletter", "zalando.de", "Marketing", "Active", 18, 212, ["one"], ["Bis zu 70% auf Sneaker", "Dein Warenkorb wartet", "Sale-Finale: Letzte Chance"]],
  ["Lieferando Angebote", "lieferando.de", "Marketing", "Active", 9, 88, ["one", "mail"], ["5 EUR Rabatt auf deine nächste Bestellung", "Heute Pizza?"]],
  ["Amazon Angebote", "amazon.de", "Marketing", "StillReceiving", 14, 301, ["link"], ["Deine Empfehlungen für Dich", "Prime Day startet bald"]],
  ["Otto Newsletter", "otto.de", "Marketing", "Active", 11, 140, ["one"], ["Möbel-Highlights", "Technik zu Bestpreisen"]],
  ["IKEA Family", "ikea.com", "Marketing", "Active", 4, 36, ["link"], ["Neu im Sortiment", "Family Angebote im Februar"]],
  ["MediaMarkt Aktionen", "mediamarkt.de", "Marketing", "Active", 12, 160, ["one", "mail"], ["Nur heute: TV-Deals", "Dein Gutschein läuft ab"]],
  ["Booking.com Deals", "booking.com", "Marketing", "Unsubscribed", 0.4, 41, ["one"], ["Genius-Angebote für dich"]],
  ["Airbnb Inspiration", "airbnb.de", "Marketing", "Active", 3, 22, ["link"], ["Wohin geht die nächste Reise?"]],
  ["Spotify Neuheiten", "spotify.com", "Newsletter", "Active", 2, 30, ["one"], ["Dein Wrapped ist da", "Neue Releases"]],
  ["Medium Daily Digest", "medium.com", "Newsletter", "Active", 25, 410, ["one", "link"], ["Stories für dich", "Wie ich meine Produktivität verdoppelte"]],
  ["Notion Updates", "notion.so", "Newsletter", "Keep", 2, 19, ["link"], ["Neu: Notion Calendar"]],
  ["GitHub Digest", "github.com", "Newsletter", "Keep", 4, 55, ["one"], ["Dein monatlicher Rückblick"]],
  ["Duolingo Erinnerung", "duolingo.com", "Marketing", "StillReceiving", 30, 520, ["one"], ["Deine Serie ist in Gefahr!", "Lena, die Eule vermisst dich"]],
  ["Coursera Empfehlungen", "coursera.org", "Marketing", "Active", 6, 74, ["one"], ["Kurse, die zu dir passen"]],
  ["Udemy Sale", "udemy.com", "Marketing", "Active", 10, 120, ["one", "link"], ["Kurse ab 12,99 EUR", "Nur 24 Stunden"]],
  ["LinkedIn Jobs", "linkedin.com", "Social", "Active", 16, 260, ["link"], ["5 neue Jobs für dich", "Wer hat dein Profil angesehen"]],
  ["Instagram Benachrichtigungen", "instagram.com", "Social", "Active", 12, 190, ["link"], ["Du hast neue Follower"]],
  ["Reddit Digest", "reddit.com", "Social", "Active", 7, 96, ["one"], ["Beliebt in r/de"]],
  ["Pinterest Ideen", "pinterest.de", "Social", "Active", 8, 70, ["one"], ["Neue Pins für dich"]],
  ["X Digest", "x.com", "Social", "Unsubscribed", 0.3, 33, ["link"], ["Was du verpasst hast"]],
  ["Strava Wochenrückblick", "strava.com", "Newsletter", "Keep", 4, 40, ["link"], ["Deine Woche in Zahlen"]],
  ["Deutsche Bahn Newsletter", "bahn.de", "Marketing", "Active", 3, 31, ["one"], ["Sparpreise ab 17,90 EUR"]],
  ["Flixbus Angebote", "flixbus.de", "Marketing", "Failed", 5, 44, ["mail"], ["Reise ab 4,99 EUR"]],
  ["Ryanair Offers", "ryanair.com", "Marketing", "Active", 6, 62, ["one"], ["Flights from 9,99 EUR"]],
  ["Skyscanner Preisalarm", "skyscanner.de", "Marketing", "Active", 5, 38, ["one", "mail"], ["Der Preis für Lissabon ist gefallen"]],
  ["HelloFresh", "hellofresh.de", "Marketing", "Active", 13, 150, ["one"], ["Dein Gratis-Kochbox Gutschein"]],
  ["Epic Games Store", "epicgames.com", "Newsletter", "Active", 4, 50, ["link"], ["Dieses Spiel gibt es gratis"]],
  ["Steam", "steampowered.com", "Newsletter", "Keep", 3, 25, ["link"], ["Dein Steam-Wochenrückblick"]],
  ["Mailchimp Kampagnen", "mailchimp.com", "Spam", "Active", 2, 14, ["link"], ["Gewinne ein iPad"]],
  ["Klarna", "klarna.com", "Transactional", "Keep", 2, 28, ["link"], ["Deine Zahlung ist fällig"]],
];

export function buildSubscriptions(): Subscription[] {
  return SUBS.map((s, i) => {
    const [name, domain, category, status, freq, count, methods, subjects] = s;
    const unsubscribe: UnsubscribeMethod[] = methods.map((m) =>
      m === "one" ? { OneClick: { url: `https://mail.${domain}/unsub/one-click?t=8f3k2` } }
        : m === "mail" ? { Mailto: { address: `unsubscribe@${domain}`, subject: "unsubscribe", body: null } }
          : { Link: { url: `https://www.${domain}/newsletter/abmelden` } });
    const mailbox = i % 3 === 2 ? W : G;
    return {
      id: uuid(i + 1, "b"), mailbox, list_key: `news@${domain}`, domain, name, sender: `newsletter@${domain}`,
      unsubscribe, message_count: count, frequency_per_month: freq, first_seen: ago(count * 4), last_seen: ago(Math.floor(rnd() * 12) + 1),
      sample_subjects: subjects, category, category_source: i % 4 === 0 ? "Ai" : "Rules", status, history: [],
    };
  });
}

export function buildRuns(): SourceRun[] {
  return [
    { id: uuid(1, "c"), source: mail(G), started_at: ago(2, 3), finished_at: ago(2, 2), items_scanned: 18420, findings: 412, new_accounts: 9, error: null },
    { id: uuid(2, "c"), source: mail(W), started_at: ago(2, 5), finished_at: ago(2, 4), items_scanned: 6210, findings: 133, new_accounts: 4, error: null },
    { id: uuid(3, "c"), source: pw("Chrome"), started_at: ago(9), finished_at: ago(9), items_scanned: 187, findings: 187, new_accounts: 12, error: null },
    { id: uuid(4, "c"), source: pw("Bitwarden"), started_at: ago(9, 2), finished_at: ago(9, 2), items_scanned: 94, findings: 94, new_accounts: 3, error: null },
    { id: uuid(5, "c"), source: "GoogleTakeout", started_at: ago(21), finished_at: ago(21), items_scanned: 1450, findings: 38, new_accounts: 5, error: null },
    { id: uuid(6, "c"), source: "FacebookExport", started_at: ago(30), finished_at: ago(30), items_scanned: 220, findings: 14, new_accounts: 3, error: null },
  ];
}

export interface GdprLetter { to: string; subject: string; body: string }

export function gdprLetterText(a: Account, fullName: string, to: string, lang: "de" | "en" = "de"): GdprLetter {
  const ident = a.identifiers.filter(Boolean).join(", ");
  const today = new Date().toLocaleDateString("de-DE", { day: "2-digit", month: "long", year: "numeric" });
  if (lang === "en") {
    return {
      to, subject: `Request for erasure of personal data (Art. 17 GDPR) – ${a.name}`,
      body: `Dear Sir or Madam,\n\nI hereby request, pursuant to Article 17 GDPR, that you erase all personal data concerning me, including my customer account at ${a.name} (${a.domain}).\n\nAccount identifiers: ${ident}\n\nPlease confirm the erasure in writing within one month of receipt of this request (Art. 12(3) GDPR).\n\nKind regards\n${fullName}\n${today}`,
    };
  }
  return {
    to, subject: `Antrag auf Löschung personenbezogener Daten (Art. 17 DSGVO) – ${a.name}`,
    body: `Sehr geehrte Damen und Herren,\n\nhiermit beantrage ich gemäß Art. 17 DSGVO die unverzügliche Löschung sämtlicher bei Ihnen gespeicherten personenbezogenen Daten, insbesondere meines Kundenkontos bei ${a.name} (${a.domain}).\n\nKennungen des Kontos: ${ident}\n\nBitte bestätigen Sie mir die vollständige Löschung innerhalb eines Monats nach Eingang dieses Schreibens schriftlich (Art. 12 Abs. 3 DSGVO).\n\nMit freundlichen Grüßen\n${fullName}\n${today}`,
  };
}
