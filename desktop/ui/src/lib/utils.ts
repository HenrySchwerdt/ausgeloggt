import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

const DAY = 86_400_000;

export function daysFromNow(iso: string): number {
  return Math.ceil((new Date(iso).getTime() - Date.now()) / DAY);
}

export function formatDate(iso: string | null | undefined): string {
  if (!iso) return "–";
  return new Date(iso).toLocaleDateString("de-DE", { day: "2-digit", month: "short", year: "numeric" });
}

export function relativeDays(iso: string | null | undefined): string {
  if (!iso) return "–";
  const d = Math.round((Date.now() - new Date(iso).getTime()) / DAY);
  if (d <= 0) return "heute";
  if (d === 1) return "gestern";
  if (d < 30) return `vor ${d} Tagen`;
  if (d < 365) {
    const m = Math.round(d / 30);
    return m === 1 ? "vor 1 Monat" : `vor ${m} Monaten`;
  }
  const y = Math.round(d / 365);
  return y === 1 ? "vor 1 Jahr" : `vor ${y} Jahren`;
}

export function deadlineLabel(iso: string): string {
  const d = daysFromNow(iso);
  if (d < 0) return `${-d} ${-d === 1 ? "Tag" : "Tage"} überfällig`;
  if (d === 0) return "heute fällig";
  if (d === 1) return "morgen fällig";
  return `in ${d} Tagen`;
}

export function isApplePrivateRelay(id: string): boolean {
  return id.endsWith("@privaterelay.appleid.com");
}

/** Truncate the middle of an Apple relay id so it stays readable. */
export function shortIdentifier(id: string): string {
  if (isApplePrivateRelay(id)) return `${id.split("@")[0]}@privaterelay…`;
  return id;
}

/** Money in minor units, e.g. (1099, "EUR") -> "10,99 €". */
export function money(minor: number | null | undefined, currency: string | null | undefined = "EUR"): string {
  if (minor === null || minor === undefined) return "–";
  try {
    return new Intl.NumberFormat("de-DE", { style: "currency", currency: currency || "EUR" }).format(minor / 100);
  } catch {
    return `${(minor / 100).toFixed(2).replace(".", ",")} ${currency ?? ""}`;
  }
}

export const nf = (n: number) => n.toLocaleString("de-DE");

export function plural(n: number, one: string, many: string): string {
  return `${nf(n)} ${n === 1 ? one : many}`;
}

/**
 * Text coming from the backend may contain dashes or hyphenated compounds. The UI language rule:
 * no em/en dashes, no hyphenated compounds. "E-Mail-Adresse verbergen" (Apple feature name) stays.
 */
export function de(s: string | null | undefined): string {
  if (!s) return "";
  const KEEP = "E-Mail-Adresse verbergen";
  const parts = s.split(KEEP);
  return parts
    .map((p) => p
      .replace(/\s[–—]\s/g, ", ")
      .replace(/[–—]/g, " ")
      .replace(/E-Mail-?/g, "Mail")
      .replace(/Zwei-Faktor-?/g, "2FA ")
      .replace(/(?<![\w.@/-])(\p{Lu}{2,})-(\p{L})(?![\w-]*[.@/])/gu, "$1 $2")
      .replace(/(?<![\w.@/-])(\p{L}{2,})-(\p{Ll}{2,})(?![\w-]*[.@/])/gu, "$1$2")
      .replace(/(?<![\w.@/-])(\p{L}{2,})-(\p{Lu}\p{Ll}+)(?![\w-]*[.@/])/gu, (_m, a: string, b: string) => `${a}${b.toLowerCase()}`)
      .replace(/ {2,}/g, " "))
    .join(KEEP);
}

export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) return de(String((e as { message: unknown }).message));
  return typeof e === "string" ? de(e) : "Unbekannter Fehler";
}

/** Throttle with trailing call dropped (activity pings). */
export function throttle<A extends unknown[]>(fn: (...a: A) => void, ms: number) {
  let last = 0;
  return (...a: A) => {
    const t = Date.now();
    if (t - last >= ms) { last = t; fn(...a); }
  };
}
