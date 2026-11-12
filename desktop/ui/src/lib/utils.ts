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
