import { useState } from "react";
import { ArrowRight, Loader2 } from "lucide-react";
import { useStore } from "@/lib/store";
import { errorMessage } from "@/lib/utils";
import { Dot, LogoMark } from "@/components/common";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Titlebar } from "@/components/titlebar";

export function passwordStrength(pw: string): { label: string; dot: string } {
  const classes = [/[a-zß-ÿ]/, /[A-ZÀ-Þ]/, /\d/, /[^A-Za-z0-9À-ÿ]/].filter((r) => r.test(pw)).length;
  if (pw.length < 10) return { label: `Noch ${10 - pw.length} ${10 - pw.length === 1 ? "Zeichen" : "Zeichen"} bis zum Minimum`, dot: "bg-s-red" };
  if (pw.length >= 16 || (pw.length >= 12 && classes >= 3)) return { label: "Stark", dot: "bg-s-green" };
  if (pw.length >= 12 || classes >= 3) return { label: "Gut. Länger ist besser.", dot: "bg-s-yellow" };
  return { label: "Schwach. Nimm mehr Zeichen oder einen Satz.", dot: "bg-s-yellow" };
}

export function UnlockScreen() {
  const exists = useStore((s) => s.vaultExists);
  const unlock = useStore((s) => s.unlock);
  const create = useStore((s) => s.create);
  const mode: "unlock" | "create" = exists ? "unlock" : "create";
  const [pw, setPw] = useState("");
  const [pw2, setPw2] = useState("");
  const [ack, setAck] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const creating = mode === "create";
  const strength = passwordStrength(pw);
  const canCreate = pw.length >= 10 && pw === pw2 && ack;

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    if (creating && pw !== pw2) return setError("Die Passwörter stimmen nicht überein.");
    setBusy(true);
    try {
      await (creating ? create(pw) : unlock(pw));
    } catch (err) {
      setError(errorMessage(err));
      setBusy(false);
    }
  }

  return (
    <div className="flex h-full flex-col bg-bg">
      <div className="[&>header]:border-transparent"><Titlebar minimal /></div>
      <main className="flex flex-1 flex-col items-center justify-center overflow-y-auto px-6 pb-16">
        <form onSubmit={submit} className="anim-pop flex w-full max-w-[340px] flex-col items-stretch gap-6">
          <div className="flex flex-col items-center gap-4 text-center">
            <LogoMark size={44} />
            <h1 className="text-xl font-semibold tracking-[-0.01em]">{creating ? "Tresor erstellen" : "Entsperren"}</h1>
          </div>

          <div className="flex flex-col gap-2">
            <Input
              type="password" autoFocus placeholder={creating ? "Hauptpasswort, mindestens 10 Zeichen" : "Hauptpasswort"} value={pw} disabled={busy}
              onChange={(e) => setPw(e.target.value)} className="h-9" aria-label="Hauptpasswort" autoComplete={creating ? "new-password" : "current-password"}
            />
            {creating && (
              <>
                <Input type="password" placeholder="Passwort wiederholen" value={pw2} disabled={busy} onChange={(e) => setPw2(e.target.value)} className="h-9" aria-label="Passwort wiederholen" autoComplete="new-password" />
                {pw && <p className="flex items-center gap-2 text-sm text-fg-muted"><Dot className={strength.dot} />{strength.label}</p>}
                <label className="mt-2 flex items-start gap-2.5 text-base text-fg-muted">
                  <Checkbox className="mt-0.5" checked={ack} onCheckedChange={(v) => setAck(v === true)} disabled={busy} />
                  <span>Ein vergessenes Passwort lässt sich nicht wiederherstellen. Dann sind die Daten im Tresor verloren.</span>
                </label>
              </>
            )}
            {error && <p className="flex items-start gap-2 text-sm text-fg"><Dot className="mt-1.5 bg-s-red" />{error}</p>}
          </div>

          <Button type="submit" variant="primary" className="h-9 w-full" disabled={busy || (creating ? !canCreate : !pw)}>
            {busy ? <Loader2 className="animate-spin" /> : null}
            {creating ? "Tresor erstellen" : "Entsperren"}
            {!busy && <ArrowRight />}
          </Button>

        </form>
      </main>
    </div>
  );
}
