import { useMemo } from "react";
import { AlertCircle, RotateCw, type LucideIcon } from "lucide-react";
import { brandPath } from "@/lib/brand";
import { useLogo } from "@/lib/logos";
import { cn } from "@/lib/utils";
import type { AccountStatus, SubscriptionStatus } from "@/lib/types";
import { Button } from "./ui/button";

/* ---------- Logo mark (matches public/icon.svg and site/assets/brand/mark.svg) ---------- */
export function LogoMark({ size = 20, className }: { size?: number; className?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" className={cn("shrink-0", className)} aria-hidden>
      <path
        d="M49 24v-3a10 10 0 0 0-10-10H19a10 10 0 0 0-10 10v22a10 10 0 0 0 10 10h20a10 10 0 0 0 10-10v-3"
        fill="none"
        strokeWidth="6.5"
        strokeLinecap="round"
        strokeLinejoin="round"
        className="stroke-fg"
      />
      <circle cx="57" cy="32" r="5.5" className="fill-[#8fc41f] dark:fill-[#c4f25a]" />
    </svg>
  );
}

/* ---------- Service logo: bundled glyph, then favicon from the backend, then a letter ---------- */
export function ServiceLogo({ name, domain, size = 24, className }: { name: string; domain: string; size?: number; className?: string }) {
  const path = useMemo(() => brandPath(name, domain), [name, domain]);
  const glyph = Math.round(size * 0.62);
  const logo = useLogo(domain, !path);
  return (
    <span
      className={cn("flex shrink-0 select-none items-center justify-center rounded-md bg-muted text-fg-muted", className)}
      style={{ width: size, height: size }}
      aria-hidden
    >
      {logo && !path ? (
        <img src={logo} alt="" width={glyph} height={glyph} className="rounded-[3px] object-contain" draggable={false} />
      ) : path ? (
        <svg width={glyph} height={glyph} viewBox="0 0 24 24" className="fill-fg">
          <path d={path} />
        </svg>
      ) : (
        <span className="font-medium leading-none" style={{ fontSize: Math.max(11, Math.round(size * 0.5)) }}>
          {name.trim().charAt(0).toUpperCase()}
        </span>
      )}
    </span>
  );
}

/* ---------- Status dot ---------- */
export const STATUS_COLOR: Record<AccountStatus, string> = {
  Discovered: "bg-s-blue",
  Confirmed: "bg-s-violet",
  Keep: "bg-s-green",
  DeletionPending: "bg-s-yellow",
  Deleted: "bg-s-gray",
  NotAnAccount: "bg-fg-faint",
  Ignored: "bg-fg-faint",
};
export const SUB_STATUS_COLOR: Record<SubscriptionStatus, string> = {
  Active: "bg-s-blue", Keep: "bg-s-green", Unsubscribed: "bg-s-gray", StillReceiving: "bg-s-red", Failed: "bg-s-red",
};

export function Dot({ className }: { className?: string }) {
  return <span className={cn("inline-block size-1.5 shrink-0 rounded-full", className)} />;
}

export function riskDot(score: number): string {
  return score >= 60 ? "bg-s-red" : score >= 30 ? "bg-s-yellow" : "bg-s-gray";
}

export function StatusLabel({ status, label, className }: { status: AccountStatus; label: string; className?: string }) {
  return (
    <span className={cn("inline-flex items-center gap-2 text-base text-fg-muted", className)}>
      <Dot className={STATUS_COLOR[status]} />
      {label}
    </span>
  );
}

/* ---------- Empty and error state ---------- */
export function EmptyState({ icon: Icon, title, description, action, className }: {
  icon: LucideIcon; title: string; description?: string; action?: React.ReactNode; className?: string;
}) {
  return (
    <div className={cn("flex flex-col items-center justify-center gap-3 px-6 py-16 text-center", className)}>
      <Icon className="size-8 text-fg-faint" strokeWidth={1.5} />
      <div className="flex max-w-[380px] flex-col gap-1">
        <p className="text-lg font-medium text-fg">{title}</p>
        {description && <p className="text-base text-fg-muted">{description}</p>}
      </div>
      {action}
    </div>
  );
}

export function ErrorState({ message, onRetry, className }: { message: string; onRetry?: () => void; className?: string }) {
  return (
    <div className={cn("flex flex-col items-center justify-center gap-3 px-6 py-16 text-center", className)}>
      <AlertCircle className="size-8 text-fg-faint" strokeWidth={1.5} />
      <p className="max-w-[380px] text-base text-fg-muted">{message}</p>
      {onRetry && <Button onClick={onRetry}><RotateCw /> Erneut versuchen</Button>}
    </div>
  );
}

/* ---------- Page header ---------- */
export function PageHeader({ title, description, actions, className }: {
  title: string; description?: React.ReactNode; actions?: React.ReactNode; className?: string;
}) {
  return (
    <div className={cn("flex items-start justify-between gap-4", className)}>
      <div className="min-w-0">
        <h1 className="text-xl font-semibold tracking-[-0.01em] text-fg">{title}</h1>
        {description && <p className="mt-0.5 text-base text-fg-muted">{description}</p>}
      </div>
      {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
    </div>
  );
}

/* ---------- Section label ---------- */
export function SectionLabel({ children, right, className }: { children: React.ReactNode; right?: React.ReactNode; className?: string }) {
  return (
    <div className={cn("flex h-6 items-center justify-between", className)}>
      <h2 className="text-sm font-medium text-fg-faint">{children}</h2>
      {right}
    </div>
  );
}

/** Key figure: small label above a large number. */
export function Figure({ label, value, hint, hintClass }: { label: string; value: React.ReactNode; hint?: React.ReactNode; hintClass?: string }) {
  return (
    <div className="min-w-0">
      <div className="text-sm text-fg-muted">{label}</div>
      <div className="tnum mt-1 text-2xl font-semibold tracking-[-0.02em]">{value}</div>
      {hint !== undefined && <div className={cn("mt-0.5 min-h-4 text-sm text-fg-faint", hintClass)}>{hint}</div>}
    </div>
  );
}

/** Row used in simple lists (no per row borders besides hairlines). */
export function ListRow({ className, ...p }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("flex min-h-11 items-center gap-3 border-b border-border px-3 last:border-b-0", className)} {...p} />;
}
