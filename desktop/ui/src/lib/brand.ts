import { BRAND_PATHS } from "./brand-icons";

/** Domains / names whose simple-icons slug differs from the natural label. */
const ALIAS: Record<string, string> = {
  "bahn": "deutschebahn", "bahn.de": "deutschebahn", "deutsche bahn": "deutschebahn",
  "x": "x", "twitter": "x",
  "zoom": "zoom", "zoomus": "zoom",
  "booking": "bookingdotcom", "booking.com": "bookingdotcom",
  "steampowered": "steam", "linktr": "linktree", "linktr.ee": "linktree",
  "disneyplus": "disneyplus", "lidl plus": "lidl", "hello fresh": "hellofresh",
  "mediamarkt": "mediamarkt", "epic games": "epicgames", "paypal": "paypal",
  "amazon": "amazon", "audible": "audible", "primevideo": "primevideo",
  "threads": "threads", "notion": "notion", "protonmail": "protonmail", "proton": "proton",
  "google": "google", "googledrive": "googledrive", "onedrive": "onedrive",
};

const clean = (s: string) => s.toLowerCase().replace(/\+/g, "plus").replace(/\./g, "dot").replace(/&/g, "and").replace(/[^a-z0-9]/g, "");

/** Registrable-domain label: "www.shop.zalando.de" -> "zalando". */
function domainLabel(domain: string): string {
  const parts = domain.toLowerCase().replace(/^www\./, "").split(".");
  if (parts.length === 1) return parts[0]!;
  const sld = new Set(["co", "com", "org", "net", "gov", "ac"]);
  const n = parts.length;
  return sld.has(parts[n - 2]!) && n >= 3 ? parts[n - 3]! : parts[n - 2]!;
}

const cache = new Map<string, string | null>();

/** Returns the SVG path (24x24 viewBox) for a brand, or null. */
export function brandPath(name: string, domain: string): string | null {
  const key = `${name}|${domain}`;
  if (cache.has(key)) return cache.get(key)!;
  const label = domainLabel(domain);
  const candidates = [
    ALIAS[domain.toLowerCase()], ALIAS[label], ALIAS[name.toLowerCase()],
    label, clean(domain), clean(name), clean(name.split(/\s+/)[0] ?? ""),
  ];
  let found: string | null = null;
  for (const c of candidates) {
    if (c && BRAND_PATHS[c]) { found = BRAND_PATHS[c]!; break; }
  }
  cache.set(key, found);
  return found;
}
