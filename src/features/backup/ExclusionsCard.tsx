import { useState } from "react";
import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import { FolderX, Undo2 } from "lucide-react";
import { api, errorMessage } from "../../lib/ipc";
import { useJob, useJobs } from "../../stores/jobs";
import { useNav } from "../../stores/nav";
import { Badge, Button, Card, ErrorNote } from "../../components/ui";
import { Modal } from "../../components/ReviewDialog";
import { useHome } from "../scanner/ScannerPage";
import { formatBytes, formatCount, formatDateTime, shortPath } from "../../lib/format";
import { artifactKindLabel } from "../dev/labels";
import type { ArtifactRemoval, DevReport, ListedExclusionState, OperationOutcome } from "../../types";

const baseName = (p: string) => p.split("/").filter(Boolean).pop() ?? p;

const stateLabel: Record<ListedExclusionState, { text: string; tone: "safe" | "review" | "neutral" }> = {
  active: { text: "Fora do backup", tone: "safe" },
  lost: { text: "Voltou ao backup", tone: "review" },
  missing: { text: "Pasta não existe mais", tone: "neutral" },
  unknown: { text: "Não foi possível saber", tone: "neutral" },
};

/**
 * Regenerable folders (node_modules, target…) only make the backup bigger. Excluding them changes a
 * Time Machine setting, so it is shown first, applied only to what is ticked, and can be undone.
 */
export function ExclusionsCard() {
  const home = useHome();
  const qc = useQueryClient();
  const go = useNav((s) => s.go);
  const job = useJob<DevReport>("dev");
  const start = useJobs((s) => s.start);
  const { data: roots = [] } = useQuery({ queryKey: ["devRoots"], queryFn: api.devRoots });
  const report = job.result;
  const running = job.status === "running";

  // Folders with versioned files are never offered: the server would refuse them anyway.
  const candidates: ArtifactRemoval[] = (report?.artifacts ?? [])
    .filter((a) => !a.notes.includes("tracked"))
    .map((a) => ({ path: a.path, repo: a.repo, kind: a.kind, size: a.size }));

  const preview = useQuery({
    queryKey: ["tmPreview", report?.scannedAt, candidates.length],
    queryFn: () => api.previewTmExclusions(candidates),
    enabled: candidates.length > 0,
    placeholderData: keepPreviousData,
    gcTime: 0,
  });
  const listed = useQuery({ queryKey: ["tmExclusions"], queryFn: api.listTmExclusions, staleTime: 0 });

  // What the user changed by hand; everything else follows the default below.
  const [overrides, setOverrides] = useState<Record<string, boolean>>({});
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [outcomes, setOutcomes] = useState<OperationOutcome[] | null>(null);
  const [undoing, setUndoing] = useState<string | null>(null);

  const items = preview.data?.items ?? [];
  // Excluding a folder from the backup removes its protection, so only folders nothing speaks against
  // (a lockfile, ignored by git) start ticked. The "Revisar" ones are the user's call.
  const safe = new Set((report?.artifacts ?? []).filter((a) => a.confidence === "safe").map((a) => a.path));
  const isTicked = (path: string, ok: boolean) => ok && (overrides[path] ?? safe.has(path));
  const chosen = items.filter((i) => isTicked(i.path, i.ok));
  const chosenBytes = chosen.reduce((s, i) => s + i.size, 0);

  const refresh = () => {
    qc.invalidateQueries({ queryKey: ["tmPreview"] });
    qc.invalidateQueries({ queryKey: ["tmExclusions"] });
  };
  const apply = async () => {
    setBusy(true);
    setError(null);
    try {
      setOutcomes(await api.applyTmExclusions(chosen.map((i) => ({ path: i.path, repo: i.repo, kind: i.kind, size: i.size }))));
      setConfirming(false);
      refresh();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const undo = async (paths: string[]) => {
    setUndoing(paths.length === 1 ? paths[0] : "*");
    setError(null);
    try {
      const res = await api.undoTmExclusions(paths);
      const failed = res.filter((r) => !r.ok);
      if (failed.length) setError(failed.map((f) => `${baseName(f.path)}: ${f.error}`).join(" · "));
      refresh();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setUndoing(null);
    }
  };

  return (
    <Card className="p-5">
      <div className="mb-1 flex items-center gap-2">
        <FolderX className="size-4 text-accent" />
        <span className="text-[13px] font-medium">Pastas que não precisam de backup</span>
      </div>
      <p className="text-[12.5px] text-ink-2">
        <span className="font-mono">node_modules</span>, <span className="font-mono">target</span> e similares voltam com um comando. Deixá-las fora do Time Machine encurta o backup e
        economiza espaço no disco de destino.
      </p>
      <p className="mt-1 text-[11.5px] text-ink-3">
        Isto muda uma configuração do Time Machine: cada pasta é marcada como excluída (sem apagar nada) e dá para desfazer aqui. A marca fica na pasta: se ela for apagada e criada de novo,
        por exemplo por uma instalação limpa, a exclusão se perde e aparece abaixo como “voltou ao backup”.
      </p>

      {error && <div className="mt-3"><ErrorNote>{error}</ErrorNote></div>}

      {!report && (
        <div className="mt-4 flex items-center gap-3">
          {roots.length === 0 ? (
            <>
              <span className="text-[12.5px] text-ink-3">Adicione uma pasta de projetos para procurar.</span>
              <Button size="sm" onClick={() => go("projects")}>
                Abrir Projetos
              </Button>
            </>
          ) : (
            <Button size="sm" busy={running} onClick={() => start("dev", "start_dev_scan", {})}>
              Procurar nas pastas de projetos
            </Button>
          )}
        </div>
      )}

      {report && candidates.length === 0 && <p className="mt-4 text-[12.5px] text-ink-2">Nenhuma pasta regenerável encontrada nos projetos.</p>}

      {preview.isError && <div className="mt-3"><ErrorNote>{errorMessage(preview.error)}</ErrorNote></div>}

      {items.length > 0 && (
        <div className="mt-4">
          <ul className="divide-y divide-line rounded-lg border border-line">
            {items.map((i) => (
              <li key={i.path}>
                <label className={`grid grid-cols-[18px_minmax(0,1fr)_auto_80px] items-center gap-3 px-3 py-2 ${i.ok ? "cursor-pointer hover:bg-surface-2" : "opacity-60"}`}>
                  <input
                    type="checkbox"
                    className="size-3.5 accent-[var(--accent)]"
                    disabled={!i.ok}
                    checked={isTicked(i.path, i.ok)}
                    onChange={() => setOverrides((cur) => ({ ...cur, [i.path]: !isTicked(i.path, i.ok) }))}
                  />
                  <div className="min-w-0">
                    <div className="flex items-center gap-1.5 truncate text-[12.5px]">
                      {artifactKindLabel[i.kind] ?? i.kind} · {baseName(i.repo)}
                      {i.ok && !safe.has(i.path) && <Badge tone="review">Revisar</Badge>}
                    </div>
                    <div className="truncate font-mono text-[11px] text-ink-3" title={i.path}>
                      {i.path.startsWith(`${i.repo}/`) ? i.path.slice(i.repo.length + 1) : shortPath(i.path, home)}
                    </div>
                  </div>
                  <div className="text-[11.5px] text-ink-3">{i.reason ?? ""}</div>
                  <span className="tabular text-right text-[12.5px]">{formatBytes(i.size)}</span>
                </label>
              </li>
            ))}
          </ul>
          <div className="mt-3 flex items-center gap-3">
            <div className="flex-1 text-[12.5px] text-ink-2">
              <span className="font-medium text-ink">{formatCount(chosen.length)}</span> {chosen.length === 1 ? "pasta" : "pastas"} ·{" "}
              <span className="tabular font-medium text-ink">{formatBytes(chosenBytes)}</span> deixam de entrar no backup
            </div>
            <Button variant="primary" disabled={chosen.length === 0} onClick={() => setConfirming(true)}>
              Excluir do backup…
            </Button>
          </div>
        </div>
      )}

      {(listed.data?.length ?? 0) > 0 && (
        <div className="mt-5">
          <div className="mb-1 flex items-center gap-2">
            <span className="text-[12.5px] font-medium">Exclusões feitas pelo app</span>
            <div className="flex-1" />
            <Button size="sm" variant="ghost" busy={undoing === "*"} icon={<Undo2 className="size-3.5" />} onClick={() => undo(listed.data!.map((x) => x.path))}>
              Desfazer todas
            </Button>
          </div>
          <ul className="divide-y divide-line rounded-lg border border-line">
            {listed.data!.map((x) => (
              <li key={x.path} className="grid grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-3 px-3 py-2">
                <div className="min-w-0">
                  <div className="truncate font-mono text-[11.5px]" title={x.path}>
                    {shortPath(x.path, home)}
                  </div>
                  <div className="text-[11px] text-ink-3">
                    {x.kind ? `${artifactKindLabel[x.kind] ?? x.kind} · ` : ""}excluída em {formatDateTime(x.addedAt)}
                    {x.state === "lost" && " · foi recriada e voltou ao backup: exclua de novo pela lista acima"}
                  </div>
                </div>
                <Badge tone={stateLabel[x.state].tone}>{stateLabel[x.state].text}</Badge>
                <Button size="sm" busy={undoing === x.path} onClick={() => undo([x.path])}>
                  Desfazer
                </Button>
              </li>
            ))}
          </ul>
        </div>
      )}

      {confirming && (
        <Modal onClose={busy ? undefined : () => setConfirming(false)} width={500}>
          <div className="p-6">
            <h2 className="text-[16px] font-semibold">Excluir do backup do Time Machine</h2>
            <p className="mt-2 text-ink-2">
              {formatCount(chosen.length)} {chosen.length === 1 ? "pasta" : "pastas"} ({formatBytes(chosenBytes)}) deixam de entrar nos próximos backups. Os arquivos continuam onde estão e backups
              antigos não mudam.
            </p>
            <p className="mt-2 text-[12px] text-ink-3">Cada pasta é conferida de novo antes. Você pode desfazer tudo nesta página.</p>
          </div>
          <div className="flex justify-end gap-2 border-t border-line px-6 py-3">
            <Button variant="ghost" onClick={() => setConfirming(false)} disabled={busy}>
              Cancelar
            </Button>
            <Button variant="primary" busy={busy} onClick={apply}>
              Excluir {formatCount(chosen.length)} {chosen.length === 1 ? "pasta" : "pastas"}
            </Button>
          </div>
        </Modal>
      )}

      {outcomes && (
        <Modal onClose={() => setOutcomes(null)} width={500}>
          <div className="p-6">
            <h2 className="text-[16px] font-semibold">
              {formatCount(outcomes.filter((o) => o.ok).length)} de {formatCount(outcomes.length)} {outcomes.length === 1 ? "pasta" : "pastas"} fora do backup
            </h2>
            {outcomes.some((o) => !o.ok) && (
              <ul className="mt-3 max-h-48 space-y-1 overflow-y-auto text-[12px]">
                {outcomes.filter((o) => !o.ok).map((o) => (
                  <li key={o.path}>
                    <span className="text-ink">{baseName(o.path)}</span> — <span className="text-ink-2">{o.error}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
          <div className="flex justify-end border-t border-line px-6 py-3">
            <Button variant="primary" onClick={() => setOutcomes(null)}>
              OK
            </Button>
          </div>
        </Modal>
      )}
    </Card>
  );
}
