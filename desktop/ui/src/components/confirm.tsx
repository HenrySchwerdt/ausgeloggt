import { useEffect, useState } from "react";
import { Button } from "./ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "./ui/dialog";

interface Ask { title: string; text?: React.ReactNode; confirm: string; destructive?: boolean; resolve(v: boolean): void }
let push: ((a: Ask) => void) | null = null;

/** Promise based confirmation dialog: `if (await confirm({...})) ...` */
export function confirm(o: { title: string; text?: React.ReactNode; confirm: string; destructive?: boolean }): Promise<boolean> {
  return new Promise((resolve) => {
    if (!push) return resolve(window.confirm(o.title));
    push({ ...o, resolve });
  });
}

export function ConfirmHost() {
  const [ask, setAsk] = useState<Ask | null>(null);
  useEffect(() => { push = setAsk; return () => { push = null; }; }, []);
  const close = (v: boolean) => { ask?.resolve(v); setAsk(null); };
  // Navigating away counts as "Abbrechen".
  useEffect(() => {
    if (!ask) return;
    const onNav = () => { ask.resolve(false); setAsk(null); };
    window.addEventListener("hashchange", onNav);
    return () => window.removeEventListener("hashchange", onNav);
  }, [ask]);
  return (
    <Dialog open={!!ask} onOpenChange={(o) => !o && close(false)}>
      <DialogContent className="max-w-[420px]">
        <DialogHeader>
          <DialogTitle>{ask?.title}</DialogTitle>
          {ask?.text && <DialogDescription>{ask.text}</DialogDescription>}
        </DialogHeader>
        <DialogFooter>
          <Button variant="ghost" onClick={() => close(false)}>Abbrechen</Button>
          <Button variant={ask?.destructive ? "destructive" : "primary"} autoFocus onClick={() => close(true)}>{ask?.confirm}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
