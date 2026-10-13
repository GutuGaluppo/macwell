import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useQueryClient } from "@tanstack/react-query";
import { Copy, FolderOpen, Link2, Search, Sparkles } from "lucide-react";
import { withBackupAck } from "../../stores/backupGate";
import { api, errorMessage } from "../../lib/ipc";
import { useJob, useJobs } from "../../stores/jobs";
import { Badge, Button, Card, EmptyState, ErrorNote, PageHeader, ProgressBar, Stat } from "../../components/ui";
import { RowActions } from "../../components/FileTable";
import { ImageThumb } from "../../components/ImageThumb";
import { Modal, SelectionBar } from "../../components/ReviewDialog";
import { WarningsNote, useHome } from "../scanner/ScannerPage";
import { CategoryIcon } from "../../lib/categories";
import { formatBytes, formatCount, formatDate, parentFolder, shortPath } from "../../lib/format";
import type { DuplicateGroup, DuplicatesResult, OperationOutcome } from "../../types";

const MODULE = "duplicates";
const PAGE = 60;

const sizes = [
  { label: "> 100 KB", value: 100e3 },
  { label: "> 1 MB", value: 1e6 },
  { label: "> 10 MB", value: 10e6 },
  { label: "> 100 MB", value: 100e6 },
];

function DuplicateProgress() {
  const job = useJob(MODULE);
  const cancel = useJobs((s) => s.cancel);
  const home = useHome();
  const stage = job.stage;
  const label = !stage
    ? "Listando arquivos"
    : stage.stage === "sample"
      ? "Comparando trechos dos arquivos"
      : "Calculando o hash completo dos candidatos";
  const fraction = stage && stage.total ? stage.done / stage.total : undefined;
  return (
    <Card className="p-6">
      <div className="flex items-start justify-between">
        <div>
          <div className="text-[15px] font-semibold">{label}</div>
          <div className="mt-0.5 text-[12px] text-ink-3">
            {stage?.stage === "full"
              ? `${formatBytes(stage.done)} de ${formatBytes(stage.total)}`
              : `${formatCount(job.progress?.files ?? 0)} arquivos em ${shortPath(job.root ?? "", home)}`}
          </div>
        </div>
        <Button size="sm" onClick={() => cancel(MODULE)}>
          Cancelar
        </Button>
      </div>
      <div className="mt-5">
        <ProgressBar value={fraction} indeterminate={fraction === undefined} />
      </div>
      <p className="mt-4 text-[11.5px] text-ink-3">
        Primeiro os arquivos são agrupados por tamanho, depois comparados por trechos do início, do meio e do fim. Só os que continuam iguais
        são lidos por inteiro.
      </p>
    </Card>
  );
}

function GroupCard({
  group,
  selected,
  toggle,
  home,
}: {
  group: DuplicateGroup;
  selected: Set<string>;
  toggle: (path: string, value: boolean) => void;
  home?: string;
}) {
  const kept = group.files.filter((f) => !selected.has(f.path)).length;
  const first = group.files[0];
  return (
    <Card className="overflow-hidden">
      <div className="flex items-center gap-3 border-b border-line bg-surface-2 px-4 py-2.5">
        <CategoryIcon entry={first} />
        <div className="min-w-0 flex-1">
          <div className="truncate font-medium">{first.name}</div>
          <div className="text-[11.5px] text-ink-3">
            {group.files.length} cópias de {formatBytes(group.size)} · {formatBytes(group.wasted)} a mais
          </div>
        </div>
      </div>
      {group.files.map((f) => {
        const checked = selected.has(f.path);
        const lastKept = !checked && kept === 1;
        return (
          <div key={f.path} className={`group grid grid-cols-[22px_minmax(0,1fr)_128px_auto_64px] items-center gap-3 border-b border-line px-4 py-2 last:border-0 ${checked ? "bg-accent-soft/50" : ""}`}>
            <input
              type="checkbox"
              className="size-3.5 accent-[var(--accent)] disabled:opacity-40"
              aria-label={`Remover ${f.name}`}
              title={lastKept ? "Pelo menos uma cópia sempre fica" : undefined}
              checked={checked}
              disabled={lastKept}
              onChange={(e) => toggle(f.path, e.target.checked)}
            />
            <div className="flex min-w-0 items-center gap-3">
              {f.category === "image" && <ImageThumb path={f.path} />}
              <div className="min-w-0 flex-1">
                <div className="truncate text-[12.5px] selectable" title={f.path}>
                  {shortPath(parentFolder(f.path), home)}/<span className="font-medium">{f.name}</span>
                </div>
                {f.hardLinks.length > 0 && (
                  <div className="flex items-center gap-1 text-[11px] text-ink-3">
                    <Link2 className="size-3" /> Também aparece como {f.hardLinks.map((l) => shortPath(l, home)).join(", ")} (mesmo arquivo, não ocupa espaço extra)
                  </div>
                )}
              </div>
            </div>
            <div className="whitespace-nowrap text-[12px] text-ink-3" title="Criado em">{formatDate(f.createdAt)}</div>
            <RowActions entry={f} />
            <div className="text-right">{checked ? <Badge tone="review">Remover</Badge> : <Badge tone="safe">Manter</Badge>}</div>
          </div>
        );
      })}
    </Card>
  );
}

function ResultDialog({ outcomes, onClose, home }: { outcomes: OperationOutcome[]; onClose: () => void; home?: string }) {
  const ok = outcomes.filter((o) => o.ok);
  const failed = outcomes.filter((o) => !o.ok);
  return (
    <Modal onClose={onClose}>
      <div className="p-6">
        <h2 className="text-[16px] font-semibold">
          {formatCount(ok.length)} {ok.length === 1 ? "cópia movida" : "cópias movidas"} para a Lixeira · {formatBytes(ok.reduce((s, o) => s + o.size, 0))}
        </h2>
        {failed.length > 0 && (
          <ul className="mt-3 max-h-48 space-y-1 overflow-y-auto rounded-lg bg-danger-soft p-3 text-[12px] selectable">
            {failed.map((f) => (
              <li key={f.path}>
                {shortPath(f.path, home)} — <span className="text-danger">{f.error}</span>
              </li>
            ))}
          </ul>
        )}
      </div>
      <div className="flex justify-end border-t border-line px-6 py-3">
        <Button variant="primary" onClick={onClose}>
          OK
        </Button>
      </div>
    </Modal>
  );
}

export function DuplicatesPage() {
  const home = useHome();
  const qc = useQueryClient();
  const job = useJob<DuplicatesResult>(MODULE);
  const start = useJobs((s) => s.start);
  const update = useJobs((s) => s.update);
  const [root, setRoot] = useState<string | null>(null);
  const [minSize, setMinSize] = useState(1e6);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [shown, setShown] = useState(PAGE);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [outcomes, setOutcomes] = useState<OperationOutcome[] | null>(null);
  const r = job.result;
  const running = job.status === "running";
  const effectiveRoot = root ?? home ?? "";

  const smartSelection = (groups: DuplicateGroup[]) => new Set(groups.flatMap((g) => g.files.filter((f) => f.selected).map((f) => f.path)));
  useEffect(() => {
    if (r) {
      setSelected(smartSelection(r.groups));
      setShown(PAGE);
    }
  }, [r?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const groups = r?.groups ?? [];
  const toRemove = useMemo(() => groups.flatMap((g) => g.files.filter((f) => selected.has(f.path)).map((f) => ({ ...f, group: g }))), [groups, selected]);
  const bytes = toRemove.reduce((s, f) => s + f.sizeLogical * (f.hardLinks.length ? 0 : 1), 0);

  const toggle = (path: string, value: boolean) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (value) next.add(path);
      else next.delete(path);
      return next;
    });

  const choose = async () => {
    const dir = await open({ directory: true, title: "Onde procurar duplicados" });
    if (typeof dir === "string") setRoot(dir);
  };

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      const request = groups
        .map((g) => ({
          all: g.files.map((f) => f.path),
          remove: g.files.filter((f) => selected.has(f.path)).map((f) => ({ path: f.path, size: f.sizeLogical, modifiedAt: f.modifiedAt ?? null })),
        }))
        .filter((g) => g.remove.length > 0);
      const out = await withBackupAck((ack) => api.moveDuplicatesToTrash(request, r?.root, ack));
      setOutcomes(out);
      setConfirm(false);
      const gone = new Set(out.filter((o) => o.ok).map((o) => o.path));
      update<DuplicatesResult>(MODULE, (res) => {
        const left = res.groups
          .map((g) => ({ ...g, files: g.files.filter((f) => !gone.has(f.path)) }))
          .filter((g) => g.files.length > 1)
          .map((g) => ({ ...g, wasted: g.size * (g.files.length - 1) }));
        return { ...res, groups: left, wastedBytes: left.reduce((s, g) => s + g.wasted, 0) };
      });
      setSelected((s) => new Set([...s].filter((p) => !gone.has(p))));
      qc.invalidateQueries({ queryKey: ["operations"] });
      qc.invalidateQueries({ queryKey: ["metrics"] });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="h-full overflow-y-auto">
      <PageHeader
        title="Duplicados"
        subtitle="Arquivos com conteúdo idêntico, comparados byte a byte (BLAKE3). Uma cópia de cada grupo sempre fica."
      />
      <div className="px-8 pb-2">
        <div className="flex flex-wrap items-center gap-3 rounded-xl glass p-4">
          <button onClick={choose} disabled={running} className="flex h-8 max-w-sm items-center gap-2 rounded-lg border border-line bg-surface-2 px-3 text-[12.5px] hover:border-accent/50">
            <FolderOpen className="size-4 shrink-0 text-accent" />
            <span className="truncate">{effectiveRoot === home ? "Pasta pessoal (~)" : shortPath(effectiveRoot, home)}</span>
          </button>
          <div className="inline-flex rounded-lg glass p-0.5">
            {sizes.map((s) => (
              <button
                key={s.value}
                onClick={() => setMinSize(s.value)}
                className={`h-7 rounded-md px-2.5 text-[12px] ${minSize === s.value ? "bg-accent font-medium text-accent-ink" : "text-ink-2 hover:text-ink"}`}
              >
                {s.label}
              </button>
            ))}
          </div>
          <div className="flex-1" />
          <Button variant="primary" icon={<Search className="size-3.5" />} disabled={running || !effectiveRoot} onClick={() => start(MODULE, "start_duplicate_scan", { root: effectiveRoot, minSize })}>
            Procurar duplicados
          </Button>
        </div>
      </div>
      <div className="space-y-4 px-8 pt-3 pb-4">
        {job.status === "failed" && <ErrorNote>{job.error}</ErrorNote>}
        {error && <ErrorNote>{error}</ErrorNote>}
        {running && <DuplicateProgress />}
        {job.status === "idle" && (
          <EmptyState icon={<Copy className="size-6" />} title="Encontre cópias que ocupam espaço à toa">
            Arquivos só no iCloud, apps e bibliotecas (Fotos, Música) ficam de fora. Hard links não contam como duplicados: são o mesmo arquivo.
          </EmptyState>
        )}
        {r && !running && (
          <>
            {r.cancelled ? (
              <div className="rounded-lg bg-surface-2 px-3 py-2 text-[12px] text-ink-2">Busca cancelada.</div>
            ) : (
              <Card className="grid grid-cols-4 gap-4 p-5">
                <Stat label="Grupos" value={formatCount(r.totalGroups)} />
                <Stat label="Espaço a mais" value={formatBytes(r.wastedBytes)} />
                <Stat label="Arquivos lidos" value={formatCount(r.files)} hint={`${formatCount(r.hashStats.fullHashed)} comparados por inteiro`} />
                <Stat label="Leitura" value={formatBytes(r.hashStats.bytesHashed)} hint="para o hash completo" />
              </Card>
            )}
            <WarningsNote module={MODULE} />
            {!r.cancelled && groups.length === 0 && <EmptyState icon={<Copy className="size-6" />} title="Nenhum duplicado encontrado" />}
            {groups.length > 0 && (
              <div className="flex items-center gap-2">
                <Button size="sm" icon={<Sparkles className="size-3.5" />} onClick={() => setSelected(smartSelection(groups))}>
                  Seleção automática
                </Button>
                <Button size="sm" variant="ghost" onClick={() => setSelected(new Set())}>
                  Desmarcar tudo
                </Button>
                <span className="text-[11.5px] text-ink-3">
                  A seleção automática mantém a cópia fora de Downloads e da Lixeira, sem "cópia" no nome, mais perto da raiz e mais antiga.
                </span>
              </div>
            )}
            {groups.slice(0, shown).map((g) => (
              <GroupCard key={g.hash} group={g} selected={selected} toggle={toggle} home={home} />
            ))}
            {groups.length > shown && (
              <div className="flex justify-center">
                <Button onClick={() => setShown((n) => n + PAGE)}>Mostrar mais {Math.min(PAGE, groups.length - shown)} grupos</Button>
              </div>
            )}
            {r.totalGroups > groups.length && (
              <p className="text-center text-[11.5px] text-ink-3">Mostrando os {formatCount(groups.length)} grupos com mais espaço a mais.</p>
            )}
            {groups.length > 0 && <SelectionBar count={toRemove.length} bytes={bytes} onReview={() => setConfirm(true)} />}
          </>
        )}
      </div>
      {confirm && (
        <Modal onClose={busy ? undefined : () => setConfirm(false)} width={520}>
          <div className="p-6">
            <h2 className="text-[16px] font-semibold">Mover {formatCount(toRemove.length)} cópias para a Lixeira?</h2>
            <p className="mt-1.5 text-ink-2">
              {formatBytes(bytes)} de {formatCount(new Set(toRemove.map((f) => f.group.hash)).size)} grupos. Em cada grupo, pelo menos uma cópia fica,
              e cópias alteradas desde a análise são puladas.
            </p>
          </div>
          <div className="flex justify-end gap-2 border-t border-line px-6 py-3">
            <Button onClick={() => setConfirm(false)} disabled={busy}>
              Cancelar
            </Button>
            <Button variant="danger" busy={busy} onClick={run}>
              Mover {formatBytes(bytes)} para a Lixeira
            </Button>
          </div>
        </Modal>
      )}
      {outcomes && <ResultDialog outcomes={outcomes} home={home} onClose={() => setOutcomes(null)} />}
    </div>
  );
}
