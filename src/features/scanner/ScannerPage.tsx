import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, Folder, FolderOpen, FolderSearch, HardDrive, Home, Download, FileText, Monitor, AppWindow, X, File } from "lucide-react";
import { api } from "../../lib/ipc";
import { useJob, useJobs } from "../../stores/jobs";
import { Button, Card, EmptyState, ErrorNote, PageHeader, ProgressBar, Stat } from "../../components/ui";
import { ScanLoader } from "../../components/loaders";
import { formatBytes, formatCount, formatDuration, shortPath } from "../../lib/format";
import type { ScanResult, StorageNode } from "../../types";
import { RowActions } from "../../components/FileTable";
import { useNav } from "../../stores/nav";

const MODULE = "scanner";

const locationMeta: Record<string, { label: string; icon: React.ReactNode }> = {
  home: { label: "Pasta pessoal", icon: <Home /> },
  downloads: { label: "Downloads", icon: <Download /> },
  documents: { label: "Documentos", icon: <FileText /> },
  desktop: { label: "Mesa", icon: <Monitor /> },
  applications: { label: "Aplicativos", icon: <AppWindow /> },
  disk: { label: "Macintosh HD", icon: <HardDrive /> },
};

export function useHome() {
  const { data } = useQuery({ queryKey: ["permissions"], queryFn: api.permissionStatus });
  return data?.home;
}

export function ScanProgressCard({ module, label }: { module: string; label?: string }) {
  const job = useJob(module);
  const cancel = useJobs((s) => s.cancel);
  const home = useHome();
  const p = job.progress;
  const elapsed = p?.elapsedMs ?? 0;
  const rate = p && elapsed > 500 ? Math.round(((p.files + p.directories) / elapsed) * 1000) : null;
  return (
    <Card className="p-6">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="text-[15px] font-semibold">{label ?? "Analisando"}</div>
          <div className="mt-0.5 truncate text-[12px] text-ink-3 selectable">{job.root ? shortPath(job.root, home) : "Preparando…"}</div>
        </div>
        <Button variant="secondary" size="sm" icon={<X className="size-3.5" />} onClick={() => cancel(module)}>
          Cancelar
        </Button>
      </div>
      <div className="mt-4">
        <ScanLoader />
      </div>
      <div className="mt-4">
        <ProgressBar indeterminate />
      </div>
      <div className="mt-5 grid grid-cols-4 gap-4">
        <Stat label="Arquivos" value={formatCount(p?.files ?? 0)} />
        <Stat label="Pastas" value={formatCount(p?.directories ?? 0)} />
        <Stat label="Tamanho lido" value={formatBytes(p?.bytesScanned ?? 0)} />
        <Stat label="Velocidade" value={rate ? `${formatCount(rate)}/s` : "—"} hint="itens por segundo" />
      </div>
      <div className="mt-4 truncate font-mono text-[11px] text-ink-3">
        {p?.waiting ? (
          <span className="font-sans text-review">
            Aguardando o macOS. Se aparecer um pedido de permissão na tela, responda para a análise continuar.
          </span>
        ) : p?.current ? (
          shortPath(p.current, home)
        ) : (
          " "
        )}
      </div>
    </Card>
  );
}

/** Why a folder was left out, from the scanner's warning message. */
function warningReason(message: string): string {
  if (message.includes("Full Disk Access")) return "requer Acesso Total ao Disco";
  if (message.includes("cloud storage")) return "armazenamento em nuvem, não analisado";
  if (message.includes("permission")) return "sem permissão de leitura";
  return message;
}

export function WarningsNote({ module }: { module: string }) {
  const job = useJob(module);
  const home = useHome();
  const [open, setOpen] = useState(false);
  const count = (job.result as { warnings?: number } | undefined)?.warnings ?? job.warnings.length;
  if (!count) return null;
  return (
    <div className="rounded-lg border border-line bg-surface-2 px-3 py-2 text-[12px] text-ink-2">
      <button className="flex w-full items-center gap-2 text-left" onClick={() => setOpen(!open)}>
        <AlertTriangle className="size-3.5 text-review" />
        <span className="flex-1">
          {formatCount(count)} {count === 1 ? "pasta ficou" : "pastas ficaram"} fora da análise e do total: protegidas pelo macOS, sem
          permissão ou na nuvem.
        </span>
        <span className="text-ink-3">{open ? "Ocultar" : "Ver"}</span>
      </button>
      {open && (
        <ul className="mt-2 max-h-40 space-y-0.5 overflow-y-auto font-mono text-[11px] text-ink-3 selectable">
          {job.warnings.map((w, i) => (
            <li key={i} className="truncate">
              {shortPath(w.path, home)} — {warningReason(w.message)}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

export function LocationPicker({ onPick, disabled }: { onPick: (path: string) => void; disabled?: boolean }) {
  const { data: locations } = useQuery({ queryKey: ["locations"], queryFn: api.suggestedLocations });
  const choose = async () => {
    const dir = await open({ directory: true, multiple: false, title: "Escolha uma pasta para analisar" });
    if (typeof dir === "string") onPick(dir);
  };
  return (
    <div className="grid grid-cols-3 gap-3">
      {(locations ?? [])
        .filter((l) => l.exists)
        .map((l) => (
          <button
            key={l.id}
            disabled={disabled}
            onClick={() => onPick(l.path)}
            className="group flex items-center gap-3 rounded-xl glass p-3.5 text-left transition hover:border-accent/50 hover:bg-accent-soft disabled:opacity-50"
          >
            <span className="grid size-9 place-items-center rounded-lg bg-surface-2 text-ink-2 group-hover:text-accent [&_svg]:size-[18px]">
              {locationMeta[l.id]?.icon ?? <Folder />}
            </span>
            <span className="min-w-0">
              <span className="block font-medium">{locationMeta[l.id]?.label ?? l.id}</span>
              <span className="block truncate text-[11px] text-ink-3">{l.path}</span>
            </span>
          </button>
        ))}
      <button
        disabled={disabled}
        onClick={choose}
        className="flex items-center gap-3 rounded-xl border border-dashed border-line p-3.5 text-left text-ink-2 transition hover:border-accent/50 hover:text-accent disabled:opacity-50"
      >
        <span className="grid size-9 place-items-center rounded-lg bg-surface-2 [&_svg]:size-[18px]">
          <FolderOpen />
        </span>
        <span className="font-medium">Escolher pasta…</span>
      </button>
    </div>
  );
}

function FolderBreakdown({ tree, home }: { tree: StorageNode; home?: string }) {
  const children = tree.children ?? [];
  const max = children[0]?.size ?? 1;
  const rows = children.slice(0, 14);
  return (
    <Card className="p-5">
      <h2 className="mb-3 text-[13px] font-semibold">Onde está o espaço</h2>
      <div className="space-y-1.5">
        {rows.map((c) => (
          <div key={c.id} className="grid grid-cols-[minmax(0,1fr)_90px] items-center gap-3">
            <div className="min-w-0">
              <div className="flex items-center gap-1.5 text-[12.5px]">
                {c.isDir ? <Folder className="size-3.5 shrink-0 text-accent" /> : <File className="size-3.5 shrink-0 text-ink-3" />}
                <span className="truncate" title={shortPath(c.path, home)}>
                  {c.name}
                </span>
              </div>
              <div className="mt-1 h-1.5 rounded-full bg-surface-3">
                <div className="h-full rounded-full bg-accent/80" style={{ width: `${Math.max(1, (c.size / max) * 100)}%` }} />
              </div>
            </div>
            <div className="tabular text-right text-[12px] text-ink-2">{formatBytes(c.size)}</div>
          </div>
        ))}
        {tree.looseFilesSize > 0 && (
          <div className="flex justify-between pt-1 text-[12px] text-ink-3">
            <span>Arquivos menores soltos nesta pasta</span>
            <span className="tabular">{formatBytes(tree.looseFilesSize)}</span>
          </div>
        )}
      </div>
    </Card>
  );
}

function LargestList({ result, home }: { result: ScanResult; home?: string }) {
  return (
    <Card className="p-5">
      <h2 className="mb-3 text-[13px] font-semibold">Maiores arquivos</h2>
      <div className="space-y-1">
        {result.largestFiles.slice(0, 14).map((f) => (
          <div key={f.path} className="group grid grid-cols-[minmax(0,1fr)_auto_72px] items-center gap-2 text-[12.5px]">
            <div className="min-w-0">
              <div className="truncate">{f.name}</div>
              <div className="truncate text-[11px] text-ink-3">{shortPath(f.path, home)}</div>
            </div>
            <RowActions entry={f} />
            <div className="tabular text-right text-ink-2">{formatBytes(f.sizeLogical)}</div>
          </div>
        ))}
      </div>
    </Card>
  );
}

export function ScannerPage() {
  const job = useJob<ScanResult>(MODULE);
  const start = useJobs((s) => s.start);
  const reset = useJobs((s) => s.reset);
  const go = useNav((s) => s.go);
  const home = useHome();
  const scan = (root: string) => start(MODULE, "start_scan", { root });
  const r = job.result;

  return (
    <div className="h-full overflow-y-auto">
      <PageHeader
        title="Scanner"
        subtitle="Descubra o que ocupa espaço em uma pasta ou no disco inteiro. A análise só lê: nenhum arquivo é alterado."
        actions={
          r &&
          job.status !== "running" && (
            <>
              <Button onClick={() => go("spaceMap")}>Ver no mapa</Button>
              <Button onClick={() => reset(MODULE)}>Nova análise</Button>
            </>
          )
        }
      />
      <div className="space-y-4 px-8 pb-10">
        {job.status === "failed" && <ErrorNote>{job.error}</ErrorNote>}

        {job.status === "running" && <ScanProgressCard module={MODULE} />}

        {(job.status === "idle" || job.status === "failed") && (
          <>
            <LocationPicker onPick={scan} />
            <EmptyState icon={<FolderSearch className="size-6" />} title="Escolha o que analisar">
              A pasta pessoal costuma ser o melhor começo. Pastas protegidas pelo macOS aparecem como avisos e ficam fora do total.
            </EmptyState>
          </>
        )}

        {r && job.status !== "running" && (
          <>
            {r.cancelled && (
              <div className="rounded-lg bg-surface-2 px-3 py-2 text-[12px] text-ink-2">Análise cancelada: os números abaixo são parciais.</div>
            )}
            <Card className="p-5">
              <div className="mb-4 flex items-baseline justify-between gap-4">
                <div className="min-w-0 truncate text-[12px] text-ink-3 selectable">{shortPath(r.root, home)}</div>
                <div className="shrink-0 text-[12px] text-ink-3">
                  {formatDuration(r.durationMs)} · {formatCount(Math.round(((r.files + r.directories) / Math.max(r.durationMs, 1)) * 1000))} itens/s
                </div>
              </div>
              <div className="grid grid-cols-4 gap-4">
                <Stat label="Tamanho" value={formatBytes(r.bytesScanned)} hint="soma dos tamanhos dos arquivos" />
                <Stat label="No disco" value={formatBytes(r.bytesAllocated)} hint="espaço alocado" />
                <Stat label="Arquivos" value={formatCount(r.files)} />
                <Stat label="Pastas" value={formatCount(r.directories)} />
              </div>
              <p className="mt-4 text-[11.5px] text-ink-3">
                "Tamanho" é o tamanho lógico. "No disco" é o espaço realmente ocupado: é menor para arquivos esparsos e para os que estão
                só no iCloud ({formatCount(r.cloudPlaceholders)} aqui), e hard links contam uma vez. Clones do APFS e snapshots podem fazer o
                espaço real liberado ser menor.
              </p>
            </Card>
            <WarningsNote module={MODULE} />
            <div className="grid grid-cols-2 gap-4">
              {r.tree && <FolderBreakdown tree={r.tree} home={home} />}
              <LargestList result={r} home={home} />
            </div>
          </>
        )}
      </div>
    </div>
  );
}
