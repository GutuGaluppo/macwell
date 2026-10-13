import { useEffect, useMemo, useState } from "react";
import { Info, PackageX } from "lucide-react";
import { withBackupAck } from "../../stores/backupGate";
import { api } from "../../lib/ipc";
import { useJob, useJobs } from "../../stores/jobs";
import { Button, Card, EmptyState, ErrorNote, PageHeader, ProgressBar } from "../../components/ui";
import { ReviewDialog, SelectionBar } from "../../components/ReviewDialog";
import { useHome } from "../scanner/ScannerPage";
import { formatBytes, formatCount, shortPath } from "../../lib/format";
import { ConfidenceBadge, kindLabel } from "./shared";
import type { OrphansResult } from "../../types";

const MODULE = "leftovers";

export function LeftoversPage() {
  const home = useHome();
  const job = useJob<OrphansResult>(MODULE);
  const start = useJobs((s) => s.start);
  const update = useJobs((s) => s.update);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [review, setReview] = useState(false);
  const r = job.result;
  const running = job.status === "running";

  useEffect(() => {
    if (r) setSelected(new Set(r.groups.flatMap((g) => g.items.filter((i) => i.selected).map((i) => i.path))));
  }, [r]); // eslint-disable-line react-hooks/exhaustive-deps

  const items = useMemo(() => (r?.groups ?? []).flatMap((g) => g.items.map((i) => ({ ...i, bundleId: g.bundleId }))), [r]);
  const chosen = items.filter((i) => selected.has(i.path));
  const bytes = chosen.reduce((s, i) => s + i.size, 0);
  const toggle = (path: string, value: boolean) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (value) next.add(path);
      else next.delete(path);
      return next;
    });

  return (
    <div className="h-full overflow-y-auto">
      <PageHeader
        title="Restos de apps"
        subtitle="Arquivos em ~/Library com o identificador de um app que não está mais instalado. Itens ambíguos nunca são selecionados sozinhos."
        actions={
          <Button variant={r ? "secondary" : "primary"} busy={running} onClick={() => start(MODULE, "start_orphan_scan", {})}>
            {r ? "Procurar de novo" : "Procurar restos"}
          </Button>
        }
      />
      <div className="space-y-4 px-8 pb-4">
        {job.status === "failed" && <ErrorNote>{job.error}</ErrorNote>}
        {running && (
          <Card className="p-5">
            <div className="mb-3 text-[13px] font-medium">{job.stage?.stage === "search" ? "Procurando em ~/Library" : "Listando os apps instalados"}</div>
            <ProgressBar indeterminate />
          </Card>
        )}
        {job.status === "idle" && (
          <EmptyState icon={<PackageX className="size-6" />} title="Encontre o que apps desinstalados deixaram">
            Os apps instalados em qualquer disco indexado pelo Spotlight são considerados. Identificadores da Apple são sempre ignorados.
          </EmptyState>
        )}
        {r && !running && (
          <>
            <div className="text-[13px] text-ink-2">
              <span className="font-semibold text-ink">{formatCount(r.groups.length)}</span> apps ·{" "}
              <span className="tabular font-semibold text-ink">{formatBytes(r.totalSize)}</span>
            </div>
            {!r.fullDiskAccess && (
              <p className="flex gap-1.5 text-[12px] text-ink-3">
                <Info className="mt-px size-3.5 shrink-0" /> Sem Acesso Total ao Disco, contêineres de apps não foram verificados.
              </p>
            )}
            {r.groups.length === 0 && <EmptyState icon={<PackageX className="size-6" />} title="Nenhum resto encontrado" />}
            {r.groups.map((g) => (
              <Card key={g.bundleId} className="overflow-hidden">
                <div className="flex items-baseline gap-2 border-b border-line bg-surface-2 px-4 py-2">
                  <span className="font-mono text-[12.5px] font-medium selectable">{g.bundleId}</span>
                  <span className="text-[12px] text-ink-3">{formatBytes(g.size)}</span>
                  {g.vendorInstalled && <span className="text-[11.5px] text-review">O fabricante tem outros apps instalados: pode ser um componente compartilhado.</span>}
                </div>
                {g.items.map((i) => (
                  <label key={i.path} className="grid cursor-pointer grid-cols-[18px_minmax(0,1fr)_auto_80px] items-center gap-3 border-b border-line px-4 py-1.5 last:border-0 hover:bg-surface-2">
                    <input type="checkbox" className="size-3.5 accent-[var(--accent)]" checked={selected.has(i.path)} onChange={(e) => toggle(i.path, e.target.checked)} />
                    <div className="min-w-0">
                      <div className="truncate text-[12.5px] selectable" title={i.path}>
                        {shortPath(i.path, home)}
                      </div>
                      <div className="text-[11px] text-ink-3">{kindLabel[i.kind]}</div>
                    </div>
                    <ConfidenceBadge confidence={i.confidence} />
                    <span className="tabular text-right text-[12.5px]">{formatBytes(i.size)}</span>
                  </label>
                ))}
              </Card>
            ))}
            {r.groups.length > 0 && <SelectionBar count={chosen.length} bytes={bytes} onReview={() => setReview(true)} />}
          </>
        )}
      </div>
      {review && (
        <ReviewDialog
          items={chosen.map((i) => ({ path: i.path, size: i.size, label: `${i.bundleId} · ${kindLabel[i.kind]}`, confidence: i.confidence }))}
          home={home}
          execute={(items) => withBackupAck((ack) => api.removeOrphans(items, ack))}
          onClose={() => setReview(false)}
          onDone={(out) => {
            const gone = new Set(out.filter((o) => o.ok).map((o) => o.path));
            update<OrphansResult>(MODULE, (res) => {
              const groups = res.groups
                .map((g) => {
                  const left = g.items.filter((i) => !gone.has(i.path));
                  return { ...g, items: left, size: left.reduce((s, i) => s + i.size, 0) };
                })
                .filter((g) => g.items.length);
              return { ...res, groups, totalSize: groups.reduce((s, g) => s + g.size, 0) };
            });
          }}
        />
      )}
    </div>
  );
}
