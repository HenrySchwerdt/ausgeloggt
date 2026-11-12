import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { ChevronDown, ExternalLink, Loader2, Server } from "lucide-react";
import { api } from "@/lib/api";
import { openUrl } from "@/lib/platform";
import { useAsync } from "@/lib/hooks";
import { MAIL_PROVIDERS, type MailProvider } from "@/lib/guides";
import { cn, errorMessage, plural } from "@/lib/utils";
import { Dot, ServiceLogo } from "./common";
import { Button } from "./ui/button";
import { Checkbox } from "./ui/checkbox";
import { DialogFooter } from "./ui/dialog";
import { Input } from "./ui/input";

export function ProviderLogo({ p, size = 28 }: { p: { name: string; domain: string; logo?: string }; size?: number }) {
  if (!p.logo && !p.domain) return <span className="flex shrink-0 items-center justify-center rounded-md bg-muted text-fg-muted" style={{ width: size, height: size }} aria-hidden><Server className="size-3.5" /></span>;
  return <ServiceLogo name={p.logo ?? p.name} domain={p.logo ? `${p.logo}.com` : p.domain || "x.invalid"} size={size} />;
}

function ProviderPicker({ onPick }: { onPick(p: MailProvider): void }) {
  return (
    <div className="flex flex-col gap-2.5">
      <p className="text-sm text-fg-faint">Wo liegt dein Postfach?</p>
      <div className="grid grid-cols-3 gap-2 max-[560px]:grid-cols-2">
        {MAIL_PROVIDERS.map((p) => (
          <button
            key={p.id} disabled={p.soon} onClick={() => onPick(p)}
            className={cn("flex h-[52px] items-center gap-2.5 rounded-lg border border-border px-3 text-left text-base font-medium transition-colors hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-transparent")}
          >
            <ProviderLogo p={p} size={24} />
            <span className="min-w-0 flex-1 truncate">{p.name}</span>
            {p.soon && <span className="text-xs font-normal text-fg-faint">bald</span>}
          </button>
        ))}
      </div>
    </div>
  );
}

type Probe = { ok: true; folders: string[] } | { ok: false; message: string; hint: string | null };

/**
 * Mailbox setup: pick a provider, follow its three steps, enter address and password, test, save.
 * Used in a dialog (Quellen) and inline (Einführung). `inline` puts the actions under the fields.
 */
export function MailboxForm({ onSaved, onCancel, inline, quiet }: {
  onSaved(id: string, address: string): void; onCancel?(): void; inline?: boolean; quiet?: boolean;
}) {
  const presets = useAsync(() => api.mail.presets(), []);
  const [provider, setProvider] = useState<MailProvider | null>(null);
  const [address, setAddress] = useState("");
  const [touched, setTouched] = useState(false);
  const [host, setHost] = useState("");
  const [port, setPort] = useState("993");
  const [user, setUser] = useState("");
  const [password, setPassword] = useState("");
  const [folders, setFolders] = useState<string[]>([]);
  const [probe, setProbe] = useState<Probe | null>(null);
  const [busy, setBusy] = useState<null | "test" | "save">(null);
  const [userEdited, setUserEdited] = useState(false);
  const [server, setServer] = useState(false);
  const [showFolders, setShowFolders] = useState(false);
  const resultRef = useRef<HTMLDivElement>(null);
  useEffect(() => { if (probe) resultRef.current?.scrollIntoView({ block: "nearest", behavior: "smooth" }); }, [probe]);


  function pick(p: MailProvider) {
    setProvider(p);
    const pr = presets.data?.find((x) => x.id === p.id);
    setHost(pr?.imap_host ?? p.host ?? ""); setPort(String(pr?.imap_port ?? p.port ?? 993));
    setFolders(pr?.default_folders ?? ["INBOX"]);
    setServer(p.id === "other" || p.id === "proton");
    setProbe(null);
  }

  const onAddress = (v: string) => {
    setAddress(v); setProbe(null);
    if (!userEdited) setUser(provider?.userIsLocalPart ? v.split("@")[0]! : v);
  };

  const addr = address.trim();
  const validAddr = /^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(addr);
  const domain = addr.split("@")[1]?.toLowerCase() ?? "";
  const mismatch = !!provider && provider.domains.length > 0 && validAddr && !provider.domains.includes(domain);
  const draft = () => ({ address: addr, imapHost: host.trim(), imapPort: Number(port) || 993, username: user.trim() || addr, password, folders });
  const valid = validAddr && host.trim() && password;

  async function test() {
    setBusy("test"); setProbe(null);
    try {
      const f = await api.mail.testDraft(draft());
      setProbe({ ok: true, folders: f });
      setFolders((cur) => { const keep = cur.filter((x) => f.includes(x)); return keep.length ? keep : f.includes("INBOX") ? ["INBOX"] : f.slice(0, 1); });
    } catch (e) {
      const code = (e as { code?: string })?.code;
      setProbe({
        ok: false, message: errorMessage(e),
        hint: code === "Auth" ? provider?.authHint ?? null : code === "Network" ? "Prüfe den Servernamen und deine Internetverbindung." : null,
      });
      if (code === "Network") setServer(true);
    } finally { setBusy(null); }
  }
  async function save() {
    setBusy("save");
    try {
      const id = await api.mail.add({ ...draft(), folders: folders.length ? folders : ["INBOX"] });
      if (!quiet) toast("Postfach hinzugefügt");
      onSaved(id, addr);
    } catch (e) { setProbe({ ok: false, message: errorMessage(e), hint: null }); } finally { setBusy(null); }
  }

  const wrap = (body: React.ReactNode, actions?: React.ReactNode) => (
    <>
      <div className={inline ? "flex flex-col gap-4" : "flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-5 pb-4"}>{body}</div>
      {actions && (inline
        ? <div className="mt-5 flex items-center justify-end gap-2">{actions}</div>
        : <DialogFooter>{actions}</DialogFooter>)}
    </>
  );

  if (!provider) {
    return wrap(<ProviderPicker onPick={pick} />, onCancel ? <Button variant="ghost" onClick={onCancel}>Abbrechen</Button> : undefined);
  }

  const ok = probe?.ok ? probe : null;
  return wrap(
    <>
      <div className="flex items-center gap-3">
        <ProviderLogo p={provider} />
        <span className="flex-1 text-lg font-medium">{provider.name}</span>
        <button onClick={() => setProvider(null)} className="text-base text-fg-muted transition-colors hover:text-fg">Anderer Anbieter</button>
      </div>

      <ol className="flex flex-col gap-2.5 rounded-lg bg-muted px-4 py-3.5">
        {provider.steps.map((s, i) => (
          <li key={s} className="flex gap-3 text-base">
            <span className="tnum w-3 shrink-0 text-fg-faint">{i + 1}</span><span>{s}</span>
          </li>
        ))}
        {provider.link && (
          <li className="pl-6">
            <Button size="sm" variant="outline" onClick={() => void openUrl(provider.link!.url)}>{provider.link.label} <ExternalLink /></Button>
          </li>
        )}
      </ol>

      <label className="flex flex-col gap-1 text-sm text-fg-muted">Mailadresse
        <Input autoFocus value={address} onChange={(e) => onAddress(e.target.value)} onBlur={() => setTouched(true)} placeholder="name@beispiel.de" inputMode="email" autoComplete="off"
          aria-invalid={touched && !!addr && !validAddr} />
        {touched && addr && !validAddr && <span className="flex items-center gap-2 text-sm text-fg"><Dot className="bg-s-red" />Das sieht nicht nach einer Mailadresse aus.</span>}
        {mismatch && <span className="flex items-center gap-2 text-sm text-fg"><Dot className="bg-s-yellow" />Diese Adresse gehört nicht zu {provider.name}. Du kannst trotzdem testen.</span>}
      </label>
      <label className="flex flex-col gap-1 text-sm text-fg-muted">{provider.steps.length > 1 && provider.id !== "posteo" ? "App Passwort oder Passwort" : "Passwort"}
        <Input type="password" value={password} placeholder="Passwort" autoComplete="off" onChange={(e) => { setPassword(e.target.value); setProbe(null); }}
          onKeyDown={(e) => e.key === "Enter" && valid && !busy && void (ok ? save() : test())} />
      </label>

      <div>
        <button onClick={() => setServer((s) => !s)} aria-expanded={server} className="flex items-center gap-1.5 text-base text-fg-muted transition-colors hover:text-fg">
          Servereinstellungen <ChevronDown className={cn("size-3.5 transition-transform", server && "rotate-180")} />
        </button>
        {server && (
          <div className="anim-fade mt-3 flex flex-col gap-3">
            <div className="grid grid-cols-[1fr_88px] gap-2">
              <label className="flex flex-col gap-1 text-sm text-fg-muted">IMAP Server<Input value={host} onChange={(e) => { setHost(e.target.value); setProbe(null); }} placeholder="imap.beispiel.de" /></label>
              <label className="flex flex-col gap-1 text-sm text-fg-muted">Port<Input value={port} onChange={(e) => setPort(e.target.value.replace(/\D/g, ""))} inputMode="numeric" /></label>
            </div>
            <label className="flex flex-col gap-1 text-sm text-fg-muted">Benutzername<Input value={user} onChange={(e) => { setUser(e.target.value); setUserEdited(true); setProbe(null); }} placeholder="meist die Mailadresse" /></label>
          </div>
        )}
      </div>

      {probe && !probe.ok && (
        <div ref={resultRef} className="anim-fade flex gap-2.5 rounded-lg border border-border px-3.5 py-3 text-base">
          <Dot className="mt-[7px] bg-s-red" />
          <div className="flex flex-col gap-1">
            <span>{probe.message}</span>
            {probe.hint && <span className="text-fg-muted">{probe.hint}</span>}
          </div>
        </div>
      )}
      {ok && (
        <div ref={resultRef} className="anim-fade flex flex-col gap-2 rounded-lg border border-border px-3.5 py-3 text-base">
          <div className="flex items-center gap-2"><Dot className="bg-s-green" />Verbindung steht, {plural(ok.folders.length, "Ordner", "Ordner")} gefunden.</div>
          <button onClick={() => setShowFolders((s) => !s)} aria-expanded={showFolders} className="flex w-fit items-center gap-1.5 text-sm text-fg-muted hover:text-fg">
            Geprüft wird: {folders.length === 1 && folders[0] === "INBOX" ? "Posteingang" : plural(folders.length, "Ordner", "Ordner")}
            <ChevronDown className={cn("size-3 transition-transform", showFolders && "rotate-180")} />
          </button>
          {showFolders && (
            <div className="flex max-h-32 flex-col overflow-y-auto">
              {ok.folders.map((f) => (
                <label key={f} className="flex h-7 items-center gap-2 text-base">
                  <Checkbox checked={folders.includes(f)} onCheckedChange={() => setFolders((cur) => cur.includes(f) ? cur.filter((x) => x !== f) : [...cur, f])} />{f}
                </label>
              ))}
            </div>
          )}
        </div>
      )}
    </>,
    <>
      {probe && !probe.ok && valid && <button className="mr-auto text-base text-fg-muted underline underline-offset-2 hover:text-fg" onClick={() => void save()} disabled={busy !== null}>Ohne Test speichern</button>}
      {onCancel && <Button variant="ghost" onClick={onCancel}>Abbrechen</Button>}
      {ok ? (
        <Button variant="primary" disabled={busy !== null || folders.length === 0} onClick={() => void save()}>{busy === "save" && <Loader2 className="animate-spin" />} Speichern und prüfen</Button>
      ) : (
        <Button variant="primary" disabled={!valid || busy !== null} onClick={() => void test()}>{busy === "test" && <Loader2 className="animate-spin" />} {probe && !probe.ok ? "Erneut testen" : "Verbindung testen"}</Button>
      )}
    </>,
  );
}
