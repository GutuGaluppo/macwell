import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { Archive, FileText, FolderOpen, Undo2 } from "lucide-react";
import { api, errorMessage } from "../../lib/ipc";
import { useNav } from "../../stores/nav";
import { useRemovalMode } from "../../lib/removalMode";
import { Badge, Button, Card, EmptyState, ErrorNote, PageHeader } from "../../components/ui";
import { Modal } from "../../components/ReviewDialog";
import { useHome } from "../scanner/ScannerPage";
import { formatBytes, formatCount, formatDate, shortPath } from "../../lib/format";
import type { OperationOutcome, QuarantineView } from "../../types";

const baseName = (p: string) => p.split("/").filter(Boolean).pop() ?? p;

type Pending = { kind: "restore" | "empty"; ids: string[] } | null;

export function QuarantinePage() {
  const home = useHome();
  const qc = useQueryClient();
  const go = useNav((s) => s.go);
  const { quarantine } = useRemovalMode();
  const { data, error } = useQuery({ queryKey: ["quarantine"], queryFn: api.listQuarantine, staleTime: 0 });
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [pending, setPending] = useState<Pending>(null);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [result, setResult] = useState<{ kind: "restore" | "empty"; outcomes: OperationOutcome[]; byPath: Map<string, string> } | null>(null);

  const items = data?.items ?? [];
  const chosen = items.filter((i) => selected.has(i.id));
  const chosenBytes = chosen.filter((i) => i.present).reduce((s, i) => s + i.size, 0);
  const days = Array.from(new Set(items.map((i) => formatDate(i.quarantinedAt))));

  const toggle = (id: string) =>
    setSelected((cur) => {
      const next = new Set(cur);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  const refresh = () => {
    for (const k of ["quarantine", "volumes", "operations", "metrics", "trash"]) qc.invalidateQueries({ queryKey: [k] });
  };

  const run = async (kind: "restore" | "empty", ids: string[], to?: string) => {
    setBusy(true);
    setProblem(null);
    try {
      const outcomes = kind === "restore" ? await api.restoreQuarantine(ids, to) : await api.emptyQuarantine(ids);
      // The outcomes of a restore are named by where the item went, so keep the id for each to retry the failures.
      const byPath = new Map(ids.map((id, n) => [outcomes[n]?.path ?? id, id]));
      setResult({ kind, outcomes, byPath });
      setPending(null);
      setSelected(new Set());
      refresh();
    } catch (e) {
      setProblem(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const retryElsewhere = async (ids: string[]) => {
    const folder = await open({ directory: true, multiple: false, title: "Escolha onde restaurar" });
    if (typeof folder === "string") await run("restore", ids, folder);
  };

  return (
    <div className="h-full overflow-y-auto">
      <PageHeader
        title="Quarentena"
        subtitle="Itens removidos em modo Quarentena ficam guardados no mesmo disco, numa pasta com a data, sem cópia. Dá para restaurar ao lugar de origem ou enviar à Lixeira. O app nunca apaga nada de vez por conta própria."
        actions={
          <>
            <Button icon={<Undo2 className="size-3.5" />} disabled={chosen.length === 0} onClick={() => setPending({ kind: "restore", ids: chosen.map((c) => c.id) })}>
              Restaurar…
            </Button>
            <Button variant="danger" disabled={chosen.length === 0} onClick={() => setPending({ kind: "empty", ids: chosen.map((c) => c.id) })}>
              Enviar à Lixeira…
            </Button>
          </>
        }
      />
      <div className="space-y-4 px-8 pb-10">
        {error && <ErrorNote>{errorMessage(error)}</ErrorNote>}

        {!quarantine && (
          <Card className="flex items-center gap-3 px-4 py-3">
            <div className="min-w-0 flex-1 text-[12.5px] text-ink-2">
              A Quarentena está desligada: o que você remove vai para a Lixeira. Ative em Ajustes para guardar os itens aqui antes de decidir.
            </div>
            <Button size="sm" onClick={() => go("settings")}>
              Abrir Ajustes
            </Button>
          </Card>
        )}

        {data && items.length === 0 && (
          <EmptyState icon={<Archive className="size-6" />} title="A Quarentena está vazia">
            Quando o modo Quarentena estiver ligado, o que você remover aparece aqui, com a origem, o tamanho e a data.
          </EmptyState>
        )}

        {items.length > 0 && (
          <>
            <Card className="grid grid-cols-2 gap-6 p-5">
              <div>
                <div className="text-[11px] font-medium uppercase tracking-wide text-ink-3">Guardado</div>
                <div className="tabular mt-1 text-[20px] font-semibold">{formatCount(items.length)} itens</div>
              </div>
              <div>
                <div className="text-[11px] font-medium uppercase tracking-wide text-ink-3">Ainda ocupa no disco</div>
                <div className="tabular mt-1 text-[20px] font-semibold">{formatBytes(data?.totalBytes ?? 0)}</div>
                <div className="mt-0.5 text-[12px] text-ink-3">libera depois de enviar à Lixeira e esvaziá-la</div>
              </div>
            </Card>
            {days.map((day) => (
              <Card key={day} className="overflow-hidden">
                <div className="flex items-center gap-3 border-b border-line px-4 py-2 text-[12px] font-medium text-ink-2">
                  {day}
                  <div className="flex-1" />
                  <Button
                    size="sm"
                    variant="ghost"
                    icon={<FileText className="size-3.5" />}
                    title="Mostrar o arquivo quarantine.csv desta pasta no Finder"
                    onClick={() => api.revealInFinder(items.find((i) => formatDate(i.quarantinedAt) === day)!.logPath)}
                  >
                    Log CSV
                  </Button>
                </div>
                <ul className="divide-y divide-line">
                  {items
                    .filter((i) => formatDate(i.quarantinedAt) === day)
                    .map((i: QuarantineView) => (
                      <li key={i.id}>
                        <label className="grid cursor-pointer grid-cols-[18px_minmax(0,1fr)_auto_80px_28px] items-center gap-3 px-4 py-2 hover:bg-surface-2">
                          <input type="checkbox" className="size-3.5 accent-[var(--accent)]" checked={selected.has(i.id)} onChange={() => toggle(i.id)} />
                          <div className="min-w-0">
                            <div className="truncate text-[12.5px] font-medium">{baseName(i.origin)}</div>
                            <div className="truncate font-mono text-[11px] text-ink-3" title={i.origin}>
                              de {shortPath(i.origin, home)}
                            </div>
                          </div>
                          <div>{!i.present && <Badge tone="review">não está mais na pasta</Badge>}</div>
                          <span className="tabular text-right text-[12.5px]">{formatBytes(i.size)}</span>
                          <button
                            type="button"
                            className="text-ink-3 hover:text-ink"
                            title="Mostrar no Finder"
                            disabled={!i.present}
                            onClick={(e) => {
                              e.preventDefault();
                              api.revealInFinder(i.dest);
                            }}
                          >
                            <FolderOpen className="size-4" />
                          </button>
                        </label>
                      </li>
                    ))}
                </ul>
              </Card>
            ))}
          </>
        )}
      </div>

      {pending && (
        <Modal onClose={busy ? undefined : () => setPending(null)} width={520}>
          <div className="p-6">
            <h2 className="text-[16px] font-semibold">{pending.kind === "restore" ? "Restaurar itens" : "Enviar itens para a Lixeira"}</h2>
            <p className="mt-2 text-ink-2">
              {pending.kind === "restore"
                ? `${formatCount(chosen.length)} ${chosen.length === 1 ? "item volta" : "itens voltam"} ao lugar de origem (${formatBytes(chosenBytes)}). Nada é sobrescrito: se já houver algo no lugar, o item continua na Quarentena e você escolhe outra pasta.`
                : `${formatCount(chosen.length)} ${chosen.length === 1 ? "item sai" : "itens saem"} da Quarentena e ${chosen.length === 1 ? "vai" : "vão"} para a Lixeira (${formatBytes(chosenBytes)}). Eles ainda podem ser recuperados pelo Finder até a Lixeira ser esvaziada.`}
            </p>
            <ul className="mt-3 max-h-40 space-y-0.5 overflow-y-auto text-[12px] text-ink-2">
              {chosen.slice(0, 8).map((c) => (
                <li key={c.id} className="truncate font-mono" title={c.origin}>
                  {shortPath(c.origin, home)}
                </li>
              ))}
              {chosen.length > 8 && <li className="text-ink-3">e mais {chosen.length - 8}</li>}
            </ul>
            {problem && <div className="mt-3"><ErrorNote>{problem}</ErrorNote></div>}
          </div>
          <div className="flex justify-end gap-2 border-t border-line px-6 py-3">
            <Button variant="ghost" disabled={busy} onClick={() => setPending(null)}>
              Cancelar
            </Button>
            <Button variant={pending.kind === "empty" ? "danger" : "primary"} busy={busy} onClick={() => run(pending.kind, pending.ids)}>
              {pending.kind === "restore" ? "Restaurar" : "Enviar à Lixeira"}
            </Button>
          </div>
        </Modal>
      )}

      {result && (
        <Modal onClose={() => setResult(null)} width={520}>
          <div className="p-6">
            <h2 className="text-[16px] font-semibold">
              {formatCount(result.outcomes.filter((o) => o.ok).length)} de {formatCount(result.outcomes.length)}{" "}
              {result.kind === "restore" ? "restaurados" : "enviados à Lixeira"}
            </h2>
            {result.outcomes.some((o) => !o.ok) && (
              <>
                <ul className="mt-3 max-h-48 space-y-1 overflow-y-auto text-[12px]">
                  {result.outcomes
                    .filter((o) => !o.ok)
                    .map((o) => (
                      <li key={o.path}>
                        <span className="text-ink">{baseName(o.path)}</span> — <span className="text-ink-2">{o.error}</span>
                      </li>
                    ))}
                </ul>
                {result.kind === "restore" && (
                  <Button
                    className="mt-3"
                    size="sm"
                    onClick={() => {
                      const failed = result.outcomes.filter((o) => !o.ok).map((o) => result.byPath.get(o.path)).filter((x): x is string => !!x);
                      setResult(null);
                      retryElsewhere(failed);
                    }}
                  >
                    Restaurar os que falharam em outra pasta…
                  </Button>
                )}
              </>
            )}
          </div>
          <div className="flex justify-end border-t border-line px-6 py-3">
            <Button variant="primary" onClick={() => setResult(null)}>
              OK
            </Button>
          </div>
        </Modal>
      )}
    </div>
  );
}
