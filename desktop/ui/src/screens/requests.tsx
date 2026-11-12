import { useMemo, useState } from "react";
import { toast } from "sonner";
import { ExternalLink, FileText, MoreHorizontal } from "lucide-react";
import { api } from "@/lib/api";
import { useStore } from "@/lib/store";
import { href, navigate } from "@/lib/router";
import { openUrl } from "@/lib/platform";
import { isOverdue, requestDays } from "@/lib/derive";
import { REQUEST_KIND_LABEL, REQUEST_STATUS_LABEL, type DataRequest, type Organization, type RequestStatus } from "@/lib/types";
import { de, deadlineLabel, errorMessage, formatDate, relativeDays } from "@/lib/utils";
import { Dot, EmptyState, PageHeader, SectionLabel } from "@/components/common";
import { PageShell, Panel } from "@/components/page";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Segmented } from "@/components/ui/segmented";
import { confirm } from "@/components/confirm";
import { openLetter } from "@/components/letter-dialog";

type Filter = "open" | "closed" | "all";
const CLOSED: RequestStatus[] = ["Answered", "Done", "Rejected"];

const statusDot = (r: DataRequest) =>
  isOverdue(r) ? "bg-s-red" : r.status === "Sent" ? "bg-s-yellow" : r.status === "Draft" ? "bg-s-gray" : r.status === "Rejected" ? "bg-s-red" : "bg-s-green";

function RequestRow({ r }: { r: DataRequest }) {
  const d = requestDays(r);
  const overdue = isOverdue(r);
  const update = (status: RequestStatus) => api.requests.update({ id: r.id, status }).catch((e) => toast.error(errorMessage(e)));
  const complaint = () => openLetter({ args: { kind: "Complaint", requestId: r.id }, target: r.target, domain: r.domain ?? undefined, parentId: r.id });
  const remind = () => openLetter({ args: { kind: "Reminder", requestId: r.id }, target: r.target, domain: r.domain ?? undefined, parentId: r.id });
  return (
    <div className="flex min-h-[52px] items-center gap-3 border-b border-border px-3 py-1.5 last:border-b-0">
      <Dot className={statusDot(r)} />
      <div className="min-w-0 flex-1">
        <div className="truncate text-base font-medium leading-[18px]">
          {r.account_id ? <a href={href({ page: "accounts", id: r.account_id })} className="hover:underline">{r.target}</a> : r.target}
          <span className="font-normal text-fg-muted">, {REQUEST_KIND_LABEL[r.kind]}</span>
        </div>
        <div className="truncate text-sm leading-4 text-fg-muted">
          {r.status === "Sent" && r.sent_at ? `Gesendet ${relativeDays(r.sent_at)}` : REQUEST_STATUS_LABEL[r.status]}{r.channel === "Post" ? ", per Post" : r.channel === "WebForm" ? ", per Formular" : ""}
        </div>
      </div>
      {d !== null && (
        <span className={`tnum hidden whitespace-nowrap text-base sm:block ${overdue ? "text-fg" : "text-fg-muted"}`} title={formatDate(r.deadline)}>{deadlineLabel(r.deadline!)}</span>
      )}
      {overdue && (r.kind === "Complaint" ? null : (
        <>
          <Button variant="secondary" onClick={remind}>Nachfassen</Button>
          <Button variant="secondary" onClick={complaint}>Beschwerde</Button>
        </>
      ))}
      <DropdownMenu>
        <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" aria-label="Weitere Aktionen"><MoreHorizontal /></Button></DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          {r.status === "Draft" && <DropdownMenuItem onSelect={() => void api.requests.update({ id: r.id, sentAt: new Date().toISOString() }).catch((e) => toast.error(errorMessage(e)))}>Als gesendet markieren</DropdownMenuItem>}
          {r.status === "Sent" && <DropdownMenuItem onSelect={() => void update("Answered")}>Antwort erhalten</DropdownMenuItem>}
          {r.status === "Sent" && <DropdownMenuItem onSelect={() => remind()}>Erinnerung schreiben</DropdownMenuItem>}
          {r.status === "Sent" && <DropdownMenuItem onSelect={() => complaint()}>Beschwerde bei der Aufsichtsbehörde</DropdownMenuItem>}
          {r.status !== "Done" && <DropdownMenuItem onSelect={() => void update("Done")}>Erledigt</DropdownMenuItem>}
          {r.status !== "Rejected" && r.status !== "Draft" && <DropdownMenuItem onSelect={() => void update("Rejected")}>Abgelehnt</DropdownMenuItem>}
          <DropdownMenuSeparator />
          <DropdownMenuItem className="text-s-red" onSelect={async () => { if (await confirm({ title: "Antrag entfernen?", text: "Nur der Eintrag in ausgeloggt wird gelöscht, nicht der Antrag beim Anbieter.", confirm: "Entfernen", destructive: true })) void api.requests.remove(r.id).catch((e) => toast.error(errorMessage(e))); }}>Entfernen</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

export function RequestsPage() {
  const snap = useStore((s) => s.snap)!;
  const reference = useStore((s) => s.reference);
  const ensure = useStore((s) => s.ensureReference);
  const [filter, setFilter] = useState<Filter>("open");
  if (!reference) void ensure();

  const rows = useMemo(() => {
    const rank = (r: DataRequest) => (isOverdue(r) ? 0 : r.status === "Sent" ? 1 : r.status === "Draft" ? 2 : 3);
    return snap.requests
      .filter((r) => filter === "all" || (filter === "closed") === CLOSED.includes(r.status))
      .sort((a, b) => rank(a) - rank(b) || (requestDays(a) ?? 999) - (requestDays(b) ?? 999) || (b.created_at > a.created_at ? 1 : -1));
  }, [snap.requests, filter]);
  const openN = snap.requests.filter((r) => !CLOSED.includes(r.status)).length;
  const overdueN = snap.requests.filter(isOverdue).length;
  const bureaus = reference?.organizations.filter((o) => o.kind === "CreditBureau") ?? [];
  const brokers = reference?.organizations.filter((o) => o.kind !== "CreditBureau") ?? [];
  const sentFor = (o: Organization) => snap.requests.find((r) => r.target === o.legal_name || r.target === o.name);

  const orgRow = (o: Organization) => {
    const sent = sentFor(o);
    const bureau = o.kind === "CreditBureau";
    return (
      <div key={o.id} className="flex min-h-[52px] items-center gap-3 border-b border-border px-3 py-1.5 last:border-b-0">
        <div className="min-w-0 flex-1">
          <div className="truncate text-base font-medium leading-[18px]">{o.name}</div>
          <div className="truncate text-sm leading-4 text-fg-muted" title={de(o.description_de)}>{de(o.description_de)}</div>
        </div>
        {sent && <span className="flex items-center gap-2 text-sm text-fg-muted"><Dot className={statusDot(sent)} />{REQUEST_STATUS_LABEL[sent.status]}</span>}
        {o.kind === "OptOutList" ? (
          o.web_form && <Button variant="secondary" onClick={() => void openUrl(o.web_form!)}>Webseite <ExternalLink /></Button>
        ) : (
          <Button variant="secondary" onClick={() => openLetter({ args: { kind: bureau ? "CreditBureauAccess" : "AddressBrokerObjection", orgId: o.id }, target: o.legal_name })}>
            <FileText /> {bureau ? "Selbstauskunft" : "Werbewiderspruch"}
          </Button>
        )}
      </div>
    );
  };

  return (
    <PageShell>
      <PageHeader
        title="Anträge"
        description={snap.requests.length ? `${openN} offen${overdueN ? `, ${overdueN} überfällig` : ""}` : undefined}
        actions={<Segmented<Filter> value={filter} onChange={setFilter} options={[{ value: "open", label: "Offen", count: openN }, { value: "closed", label: "Beantwortet", count: snap.requests.length - openN }, { value: "all", label: "Alle" }]} />}
      />
      <Panel>
        {rows.length === 0 ? (
          <EmptyState icon={FileText} className="py-14"
            title={snap.requests.length === 0 ? "Noch keine Anträge" : "Keine Anträge in dieser Ansicht"}
            description={snap.requests.length === 0 ? "Löschanträge, Auskünfte und Kündigungen schreibst du im jeweiligen Account. Hier behältst du die Fristen im Blick." : undefined}
            action={snap.requests.length === 0 ? <Button onClick={() => navigate({ page: "cleanup" })}>Zu Löschen und Kündigen</Button> : undefined} />
        ) : rows.map((r) => <RequestRow key={r.id} r={r} />)}
      </Panel>

      {bureaus.length > 0 && (
        <section>
          <SectionLabel>Auskunfteien</SectionLabel>
          <Panel className="mt-1">{bureaus.map(orgRow)}</Panel>
        </section>
      )}
      {brokers.length > 0 && (
        <section>
          <SectionLabel>Adresshändler und Sperrlisten</SectionLabel>
          <Panel className="mt-1">{brokers.map(orgRow)}</Panel>
        </section>
      )}
    </PageShell>
  );
}
