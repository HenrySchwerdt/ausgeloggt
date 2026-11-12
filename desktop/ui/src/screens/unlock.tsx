import { useState } from "react";
import { ArrowRight, Loader2, ShieldCheck } from "lucide-react";
import { useStore } from "@/lib/store";
import { LogoMark } from "@/components/common";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Titlebar } from "@/components/titlebar";

export function UnlockScreen() {
  const exists = useStore((s) => s.vaultExists);
  const unlock = useStore((s) => s.unlock);
  const create = useStore((s) => s.create);
  const [mode, setMode] = useState<"unlock" | "create">(exists ? "unlock" : "create");
  const [pw, setPw] = useState("");
  const [pw2, setPw2] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const creating = mode === "create";

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    if (creating && pw !== pw2) return setError("Die Passwörter stimmen nicht überein.");
    setBusy(true);
    try {
      await (creating ? create(pw) : unlock(pw));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setBusy(false);
    }
  }

  return (
    <div className="flex h-full flex-col bg-bg">
      <div className="[&>header]:border-transparent"><Titlebar minimal /></div>
      <main className="flex flex-1 flex-col items-center justify-center px-6 pb-16">
        <form onSubmit={submit} className="anim-pop flex w-full max-w-[320px] flex-col items-stretch gap-6">
          <div className="flex flex-col items-center gap-4 text-center">
            <LogoMark size={44} />
            <div>
              <h1 className="text-xl font-semibold tracking-[-0.01em]">{creating ? "Tresor erstellen" : "Spurlos entsperren"}</h1>
              <p className="mt-1 text-base text-fg-muted">
                {creating ? "Wähle ein Master-Passwort. Es kann nicht wiederhergestellt werden." : "Gib dein Master-Passwort ein."}
              </p>
            </div>
          </div>

          <div className="flex flex-col gap-2">
            <Input
              type="password" autoFocus placeholder="Master-Passwort" value={pw} disabled={busy}
              onChange={(e) => setPw(e.target.value)} className="h-9" aria-label="Master-Passwort"
            />
            {creating && (
              <Input
                type="password" placeholder="Passwort wiederholen" value={pw2} disabled={busy}
                onChange={(e) => setPw2(e.target.value)} className="h-9" aria-label="Passwort wiederholen"
              />
            )}
            {error && (
              <p className="flex items-center gap-2 text-sm text-s-red">
                <span className="size-1.5 rounded-full bg-s-red" /> {error}
              </p>
            )}
          </div>

          <Button type="submit" variant="primary" className="h-9 w-full" disabled={busy || !pw}>
            {busy ? <Loader2 className="animate-spin" /> : null}
            {creating ? "Tresor erstellen" : "Entsperren"}
            {!busy && <ArrowRight />}
          </Button>

          <button
            type="button" className="text-center text-base text-fg-muted transition-colors hover:text-fg"
            onClick={() => { setMode(creating ? "unlock" : "create"); setError(null); }}
          >
            {creating ? "Ich habe bereits einen Tresor" : "Neuen Tresor erstellen"}
          </button>
        </form>
      </main>
      <footer className="flex items-center justify-center gap-1.5 pb-5 text-sm text-fg-faint">
        <ShieldCheck className="size-3.5" /> Alle Daten bleiben verschlüsselt auf diesem Gerät.
      </footer>
    </div>
  );
}
