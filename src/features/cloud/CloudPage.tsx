import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Cloud, CloudDownload, CloudOff, FolderOpen, Info } from "lucide-react";
import { api, errorMessage } from "../../lib/ipc";
import { useJob, useJobs } from "../../stores/jobs";
import { Button, Card, EmptyState, ErrorNote, PageHeader, ProgressBar } from "../../components/ui";
import { CloudLoader } from "../../components/loaders";
import { Modal } from "../../components/ReviewDialog";
import { useHome } from "../scanner/ScannerPage";
import { formatBytes, formatCount, shortPath } from "../../lib/format";
import type { CloudFolder, CloudProvider, CloudUsage, EvictOutcome } from "../../types";

const MODULE = "cloud";

const providerHelp: Record<CloudProvider, string> = {
  iCloud: "",
  googleDrive: "No Google Drive, clique com o botão direito no arquivo ou pasta e escolha Disponível off-line → desmarcar, ou use Remover download no Finder.",
  dropbox: "No Dropbox, clique com o botão direito e escolha Tornar somente on-line.",
  oneDrive: "No OneDrive, clique com o botão direito e escolha Liberar espaço.",
  box: "No Box, clique com o botão direito e escolha Remover download.",
  other: "Use a opção do próprio app de nuvem para deixar os arquivos só na nuvem (em geral, Remover download no Finder).",
};

function Usage({ usage, home }: { usage: CloudUsage; home?: string }) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<EvictOutcome[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<Set<string>>(new Set());
  const localPct = usage.totalBytes ? usage.localBytes / usage.totalBytes : 0;
  const files = usage.largestLocal.filter((f) => !done.has(f.path));
  const bytes = files.filter((f) => selected.has(f.path)).reduce((s, f) => s + f.sizeLogical, 0);

  const evict = async () => {
    setBusy(true);
    setError(null);
    try {
      const out = await api.evictICloud([...selected]);
      setResult(out);
      setDone((d) => new Set([...d, ...out.filter((o) => o.ok).map((o) => o.path)]));
      setSelected(new Set());
      setConfirm(false);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-4">
      <Card className="p-5">
        <div className="grid grid-cols-4 gap-4">
          <div>
            <div className="text-[11px] font-medium uppercase tracking-wide text-ink-3">Na nuvem</div>
            <div className="tabular mt-1 text-[20px] font-semibold">{formatBytes(usage.totalBytes)}</div>
            <div className="text-[12px] text-ink-3">{formatCount(usage.files)} arquivos</div>
          </div>
          <div>
            <div className="text-[11px] font-medium uppercase tracking-wide text-ink-3">Neste Mac</div>
            <div className="tabular mt-1 text-[20px] font-semibold">{formatBytes(usage.localBytes)}</div>
            <div className="text-[12px] text-ink-3">baixados</div>
          </div>
          <div>
            <div className="text-[11px] font-medium uppercase tracking-wide text-ink-3">Só na nuvem</div>
            <div className="tabular mt-1 text-[20px] font-semibold">{formatCount(usage.cloudOnlyFiles)}</div>
            <div className="text-[12px] text-ink-3">arquivos sem cópia local</div>
          </div>
          <div className="self-end">
            <ProgressBar value={localPct} />
            <div className="mt-1 text-[11.5px] text-ink-3">{Math.round(localPct * 100)}% baixado</div>
          </div>
        </div>
      </Card>
      {!usage.canEvict && (
        <div className="flex gap-2 rounded-lg bg-surface-2 px-3 py-2 text-[12.5px] text-ink-2">
          <Info className="mt-0.5 size-4 shrink-0 text-ink-3" /> {providerHelp[usage.provider]}
        </div>
      )}
      {error && <ErrorNote>{error}</ErrorNote>}
      {result && result.some((r) => !r.ok) && (
        <ErrorNote>
          {result.filter((r) => !r.ok).length} arquivos não puderam ficar só na nuvem (em geral porque ainda não terminaram de enviar): {result.find((r) => !r.ok)?.error}
        </ErrorNote>
      )}
      {files.length > 0 && (
        <div>
          <h2 className="mb-2 text-[13px] font-semibold">Maiores arquivos baixados</h2>
          <Card className="divide-y divide-line">
            {files.map((f) => (
              <label key={f.path} className={`grid grid-cols-[18px_minmax(0,1fr)_80px_28px] items-center gap-3 px-4 py-1.5 ${usage.canEvict ? "cursor-pointer hover:bg-surface-2" : ""}`}>
                {usage.canEvict ? (
                  <input
                    type="checkbox"
                    className="size-3.5 accent-[var(--accent)]"
                    checked={selected.has(f.path)}
                    onChange={() =>
                      setSelected((prev) => {
                        const next = new Set(prev);
                        if (next.has(f.path)) next.delete(f.path);
                        else next.add(f.path);
                        return next;
                      })
                    }
                  />
                ) : (
                  <span />
                )}
                <div className="min-w-0">
                  <div className="truncate text-[12.5px]">{f.name}</div>
                  <div className="truncate text-[11px] text-ink-3">{shortPath(f.path, home)}</div>
                </div>
                <span className="tabular text-right text-[12.5px]">{formatBytes(f.sizeLogical)}</span>
                <button className="text-ink-3 hover:text-ink" title="Mostrar no Finder" onClick={(e) => { e.preventDefault(); api.revealInFinder(f.path); }}>
                  <FolderOpen className="size-3.5" />
                </button>
              </label>
            ))}
          </Card>
        </div>
      )}
      {usage.canEvict && files.length > 0 && (
        <div className="sticky bottom-0 -mx-8 flex items-center gap-3 glass-bar px-8 py-3">
          <div className="flex-1 text-[12.5px] text-ink-2">
            {selected.size ? `${formatCount(selected.size)} arquivos · ${formatBytes(bytes)} liberados neste Mac` : "Escolha arquivos para deixar só no iCloud."}
          </div>
          <Button variant="primary" icon={<CloudOff className="size-3.5" />} disabled={!selected.size} onClick={() => setConfirm(true)}>
            Remover download…
          </Button>
        </div>
      )}
      {confirm && (
        <Modal onClose={busy ? undefined : () => setConfirm(false)} width={460}>
          <div className="p-6">
            <h2 className="text-[16px] font-semibold">Deixar {formatCount(selected.size)} arquivos só no iCloud?</h2>
            <p className="mt-1.5 text-ink-2">
              Libera {formatBytes(bytes)} neste Mac. Os arquivos continuam no iCloud Drive e em todos os aparelhos, e baixam de novo quando você
              abrir. Nada é apagado.
            </p>
          </div>
          <div className="flex justify-end gap-2 border-t border-line px-6 py-3">
            <Button onClick={() => setConfirm(false)} disabled={busy}>
              Cancelar
            </Button>
            <Button variant="primary" busy={busy} onClick={evict}>
              Remover download
            </Button>
          </div>
        </Modal>
      )}
    </div>
  );
}

export function CloudPage() {
  const home = useHome();
  const { data: folders } = useQuery({ queryKey: ["cloudFolders"], queryFn: api.cloudFolders });
  const job = useJob<CloudUsage>(MODULE);
  const start = useJobs((s) => s.start);
  const [current, setCurrent] = useState<CloudFolder | null>(null);
  const running = job.status === "running";
  const measure = (f: CloudFolder) => {
    setCurrent(f);
    start(MODULE, "start_cloud_usage", { path: f.path });
  };

  return (
    <div className="h-full overflow-y-auto">
      <PageHeader
        title="Nuvem"
        subtitle="Quanto de cada pasta sincronizada está baixado neste Mac. Deixar arquivos só na nuvem libera espaço sem apagar nada."
      />
      <div className="space-y-4 px-8 pb-4">
        {folders && folders.length === 0 && (
          <EmptyState icon={<Cloud className="size-6" />} title="Nenhuma pasta de nuvem encontrada">
            iCloud Drive, Google Drive, Dropbox e OneDrive aparecem aqui quando estão configurados neste Mac.
          </EmptyState>
        )}
        <div className="grid grid-cols-3 gap-3">
          {folders?.map((f) => (
            <button
              key={f.path}
              onClick={() => measure(f)}
              disabled={running}
              className={`flex items-center gap-3 rounded-xl border p-4 text-left transition ${current?.path === f.path ? "border-accent bg-accent-soft" : "glass hover:border-accent/50"}`}
            >
              <span className="grid size-9 place-items-center rounded-lg bg-surface-2 text-accent">
                <CloudDownload className="size-[18px]" />
              </span>
              <span className="min-w-0">
                <span className="block truncate font-medium">{f.name}</span>
                <span className="block truncate text-[11px] text-ink-3">{shortPath(f.path, home)}</span>
              </span>
            </button>
          ))}
        </div>
        {job.status === "failed" && <ErrorNote>{job.error}</ErrorNote>}
        {running && (
          <Card className="p-5">
            <CloudLoader />
            <div className="mb-3 text-[13px] font-medium">
              Medindo {current?.name} · {formatCount(job.progress?.files ?? 0)} arquivos
            </div>
            <ProgressBar indeterminate />
            <p className="mt-3 text-[11.5px] text-ink-3">Só lê informações dos arquivos: nada é baixado.</p>
          </Card>
        )}
        {job.result && !running && <Usage key={job.result.path} usage={job.result} home={home} />}
      </div>
    </div>
  );
}
