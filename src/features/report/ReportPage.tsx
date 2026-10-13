import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { save } from "@tauri-apps/plugin-dialog";
import { CheckCircle2, FileDown, RefreshCw } from "lucide-react";
import { api, errorMessage } from "../../lib/ipc";
import { useNav } from "../../stores/nav";
import { Badge, Button, Card, ErrorNote, PageHeader } from "../../components/ui";
import { Modal } from "../../components/ReviewDialog";
import { formatBytes, formatCount, formatDateTime } from "../../lib/format";
import { statusBadge, statusTone } from "../backup/labels";
import type { AppError, Exported, ReportFormat } from "../../types";

const PERIODS = [7, 30, 90, 365];
const nextSection: Record<string, "backup" | "projects" | "quarantine" | "smartCare"> = {
  backup: "backup",
  snapshot: "projects",
  pushSnapshots: "projects",
  quarantine: "quarantine",
  exclusions: "backup",
  autoBackup: "backup",
  none: "smartCare",
};
const nextLabel: Record<string, string> = { backup: "Abrir Backup", snapshot: "Abrir Projetos", pushSnapshots: "Abrir Projetos", quarantine: "Abrir Quarentena", exclusions: "Abrir Backup", autoBackup: "Abrir Backup", none: "Abrir Cuidado inteligente" };

const FORMATS: Record<ReportFormat, { label: string; ext: string; filter: string }> = {
  markdown: { label: "Salvar como Markdown…", ext: "md", filter: "Markdown" },
  csv: { label: "Salvar como CSV…", ext: "csv", filter: "CSV" },
};

export function ReportPage() {
  const go = useNav((s) => s.go);
  const [days, setDays] = useState(30);
  const { data: report, error, isFetching, refetch } = useQuery({ queryKey: ["report", days], queryFn: () => api.generateReport(days), staleTime: 0 });
  const [saving, setSaving] = useState<ReportFormat | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [saved, setSaved] = useState<Exported | null>(null);
  const [replace, setReplace] = useState<{ format: ReportFormat; path: string } | null>(null);
  const [showAll, setShowAll] = useState(false);

  const write = async (format: ReportFormat, path: string, overwrite: boolean) => {
    setSaving(format);
    setProblem(null);
    try {
      setSaved(await api.exportReport(format, path, overwrite, days));
      setReplace(null);
    } catch (e) {
      if ((e as AppError)?.kind === "exists") setReplace({ format, path });
      else setProblem(errorMessage(e));
    } finally {
      setSaving(null);
    }
  };

  const choose = async (format: ReportFormat) => {
    const { ext, filter } = FORMATS[format];
    const stamp = new Date().toISOString().slice(0, 10);
    const path = await save({ defaultPath: `relatorio-organizemymac-${stamp}.${ext}`, filters: [{ name: filter, extensions: [ext] }], title: "Salvar o relatório" });
    if (typeof path === "string") await write(format, path, false);
  };

  const lines = report ? (showAll ? report.recent : report.recent.slice(0, 10)) : [];

  return (
    <div className="h-full overflow-y-auto">
      <PageHeader
        title="Relatório"
        subtitle="O que o app fez, o que ainda falta e a próxima decisão. É montado com os registros guardados neste Mac e só é gravado onde você escolher; nada é enviado a lugar nenhum."
        actions={
          <>
            <select className="glass h-8 rounded-lg px-2 text-[13px]" value={days} onChange={(e) => setDays(Number(e.target.value))} aria-label="Período">
              {PERIODS.map((d) => (
                <option key={d} value={d}>
                  Últimos {d} dias
                </option>
              ))}
            </select>
            <Button icon={<RefreshCw className="size-3.5" />} busy={isFetching} onClick={() => refetch()}>
              Atualizar
            </Button>
          </>
        }
      />
      <div className="space-y-4 px-8 pb-10">
        {error && <ErrorNote>{errorMessage(error)}</ErrorNote>}
        {problem && <ErrorNote>{problem}</ErrorNote>}
        {!report && !error && <div className="text-ink-3">Montando o relatório…</div>}

        {report && (
          <>
            <Card className="p-5">
              <div className="text-[11px] font-medium uppercase tracking-wide text-ink-3">Próxima decisão</div>
              <p className="mt-1 text-[16px] font-semibold">{report.next.text}</p>
              <div className="mt-3 flex flex-wrap items-center gap-2">
                <Button size="sm" onClick={() => go(nextSection[report.next.code] ?? "smartCare")}>
                  {nextLabel[report.next.code] ?? "Abrir"}
                </Button>
                <div className="flex-1" />
                {(Object.keys(FORMATS) as ReportFormat[]).map((f) => (
                  <Button key={f} size="sm" icon={<FileDown className="size-3.5" />} busy={saving === f} onClick={() => choose(f)}>
                    {FORMATS[f].label}
                  </Button>
                ))}
              </div>
              {saved && (
                <div className="mt-3 flex items-center gap-2 text-[12px] text-ink-2">
                  <CheckCircle2 className="size-4 shrink-0 text-safe" />
                  <span className="min-w-0 truncate font-mono selectable" title={saved.path}>
                    {saved.path}
                  </span>
                  <span className="shrink-0 text-ink-3">{formatBytes(saved.bytes)}</span>
                  <Button size="sm" variant="ghost" onClick={() => api.revealInFinder(saved.path)}>
                    Mostrar no Finder
                  </Button>
                </div>
              )}
            </Card>

            <Card className="p-5">
              <div className="mb-2 flex items-center gap-2 text-[13px] font-medium">
                O que falta
                {report.pending.length === 0 && <Badge tone="safe">Nada pendente</Badge>}
              </div>
              {report.pending.length > 0 && (
                <ul className="space-y-3">
                  {report.pending.map((p) => (
                    <li key={p.code}>
                      <div className="text-[12.5px]">
                        <span className="font-medium">{p.title}</span>
                        {p.bytes > 0 && <span className="text-ink-3"> · {formatBytes(p.bytes)}</span>}
                      </div>
                      <div className="text-[12px] text-ink-3">{p.detail}</div>
                      {p.items.length > 0 && (
                        <ul className="mt-1 space-y-0.5">
                          {p.items.map((it, i) => (
                            <li key={i} className="text-[11.5px] text-ink-2">
                              <span className="font-mono selectable">{it.text}</span>
                              {it.note && <span className="text-ink-3"> — {it.note}</span>}
                            </li>
                          ))}
                          {p.count > p.items.length && <li className="text-[11.5px] text-ink-3">… e mais {p.count - p.items.length}</li>}
                        </ul>
                      )}
                    </li>
                  ))}
                </ul>
              )}
              <div className="mt-3 flex flex-wrap items-center gap-2 border-t border-line pt-3 text-[12px] text-ink-2">
                Backup:
                <Badge tone={statusTone[report.backup.status]}>{statusBadge[report.backup.status]}</Badge>
                {report.backup.lastBackupMs && <span>último em {formatDateTime(report.backup.lastBackupMs)}</span>}
                {report.backup.autoBackup === false && <Badge tone="review">automático desligado</Badge>}
              </div>
            </Card>

            <Card className="p-5">
              <div className="mb-2 text-[13px] font-medium">O que foi feito nos últimos {report.periodDays} dias</div>
              {report.groups.length === 0 ? (
                <p className="text-[12.5px] text-ink-3">Nenhuma ação no período.</p>
              ) : (
                <>
                  <table className="w-full text-[12.5px]">
                    <thead>
                      <tr className="text-left text-[11px] uppercase tracking-wide text-ink-3">
                        <th className="pb-1 font-medium">Ação</th>
                        <th className="pb-1 text-right font-medium">Tentativas</th>
                        <th className="pb-1 text-right font-medium">Concluídas</th>
                        <th className="pb-1 text-right font-medium">Tamanho</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-line">
                      {report.groups.map((g) => (
                        <tr key={g.action}>
                          <td className="py-1.5">{g.label}</td>
                          <td className="tabular py-1.5 text-right">{formatCount(g.total)}</td>
                          <td className="tabular py-1.5 text-right">{formatCount(g.ok)}</td>
                          <td className="tabular py-1.5 text-right">{formatBytes(g.bytes)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                  <ul className="mt-4 divide-y divide-line rounded-lg border border-line">
                    {lines.map((l, i) => (
                      <li key={i} className="grid grid-cols-[110px_minmax(0,1fr)_80px_auto] items-center gap-3 px-3 py-1.5 text-[12px]">
                        <span className="text-ink-3">{formatDateTime(l.whenMs)}</span>
                        <div className="min-w-0">
                          <div className="truncate">{l.label}</div>
                          <div className="truncate font-mono text-[11px] text-ink-3" title={l.path}>
                            {l.path}
                          </div>
                        </div>
                        <span className="tabular text-right">{formatBytes(l.size)}</span>
                        {l.ok ? <Badge tone="safe">ok</Badge> : <Badge tone="danger">falhou</Badge>}
                      </li>
                    ))}
                  </ul>
                  <div className="mt-2 flex items-center gap-2 text-[11.5px] text-ink-3">
                    {report.recent.length > 10 && (
                      <Button size="sm" variant="ghost" onClick={() => setShowAll((v) => !v)}>
                        {showAll ? "Mostrar menos" : `Ver as ${report.recent.length} ações`}
                      </Button>
                    )}
                    {report.recentOmitted > 0 && <span>O arquivo traz até 200 ações; há mais {report.recentOmitted} no período, contadas nos totais.</span>}
                  </div>
                </>
              )}
              {report.snapshots.length > 0 && (
                <div className="mt-4">
                  <div className="mb-1 text-[12px] font-medium text-ink-2">Snapshots</div>
                  <ul className="space-y-0.5 text-[12px]">
                    {report.snapshots.map((s, i) => (
                      <li key={i} className="flex items-center gap-2">
                        <span className="font-mono">{s.refName}</span>
                        <span className="truncate text-ink-3">em {s.repo}</span>
                        <Badge tone={s.pushed ? "safe" : "review"}>{s.pushed ? "enviado" : "só neste Mac"}</Badge>
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </Card>
          </>
        )}
      </div>

      {replace && (
        <Modal onClose={() => setReplace(null)} width={480}>
          <div className="p-6">
            <h2 className="text-[16px] font-semibold">Substituir o arquivo?</h2>
            <p className="mt-2 text-ink-2">
              Já existe um arquivo em <span className="break-all font-mono text-[12px] text-ink">{replace.path}</span>. O app só o substitui se você confirmar.
            </p>
          </div>
          <div className="flex justify-end gap-2 border-t border-line px-6 py-3">
            <Button variant="ghost" onClick={() => setReplace(null)}>
              Cancelar
            </Button>
            <Button variant="danger" busy={saving !== null} onClick={() => write(replace.format, replace.path, true)}>
              Substituir
            </Button>
          </div>
        </Modal>
      )}
    </div>
  );
}
