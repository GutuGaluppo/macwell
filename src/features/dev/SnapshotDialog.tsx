import { useEffect, useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { CheckCircle2, Info, ShieldAlert } from "lucide-react";
import { Badge, Button, ErrorNote } from "../../components/ui";
import { Modal } from "../../components/ReviewDialog";
import { api, errorMessage } from "../../lib/ipc";
import { formatBytes, formatCount, shortPath } from "../../lib/format";
import type { RepoReport, SnapshotResult } from "../../types";
import { blockerText, secretKindLabel } from "./labels";

/**
 * Preview first, then one clearly labelled action. The plan comes from the Rust side and is derived
 * again when the snapshot is created, so what is shown here is what can happen, never more.
 */
export function SnapshotDialog({
  repo,
  home,
  onClose,
  onCreated,
  onPush,
}: {
  repo: RepoReport;
  home?: string;
  onClose: () => void;
  onCreated: (result: SnapshotResult) => void;
  onPush: (refName: string) => void;
}) {
  const [excludes, setExcludes] = useState<string[]>([]);
  // Everything that ever needed a decision, so a path stays listed (and can be un-excluded) after it leaves the plan.
  const [candidates, setCandidates] = useState<Record<string, string[]>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<SnapshotResult | null>(null);

  const query = useQuery({
    queryKey: ["snapshotPlan", repo.path, excludes],
    queryFn: () => api.planWipSnapshot(repo.path, excludes),
    placeholderData: keepPreviousData,
    gcTime: 0,
    retry: false,
  });
  const plan = query.data;

  useEffect(() => {
    if (!plan) return;
    setCandidates((cur) => {
      const next = { ...cur };
      const add = (path: string, why: string) => {
        next[path] = Array.from(new Set([...(next[path] ?? []), why]));
      };
      plan.secrets.forEach((s) => add(s.path, secretKindLabel[s.kind]));
      plan.blockers.forEach((b) => b.code === "fileTooBig" && add(b.path, `arquivo grande (${formatBytes(b.size)})`));
      return next;
    });
  }, [plan]);

  const toggle = (path: string) => setExcludes((cur) => (cur.includes(path) ? cur.filter((p) => p !== path) : [...cur, path]));
  const blocked = !plan || plan.blockers.length > 0 || query.isFetching;
  const decisions = Object.entries(candidates);

  const create = async () => {
    setBusy(true);
    setError(null);
    try {
      const res = await api.createWipSnapshot(repo.path, excludes);
      setResult(res);
      onCreated(res);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  if (result) {
    return (
      <Modal onClose={onClose}>
        <div className="p-6">
          <div className="flex items-center gap-2 text-[16px] font-semibold">
            <CheckCircle2 className="size-5 text-safe" /> Snapshot criado
          </div>
          <p className="mt-2 text-ink-2">
            O trabalho em andamento de <span className="font-medium text-ink">{repo.path.split("/").pop()}</span> está salvo no branch local{" "}
            <span className="font-mono text-ink selectable">{result.refName}</span> (commit <span className="font-mono">{result.commit.slice(0, 10)}</span>). Seus
            arquivos, o branch atual e o índice não foram tocados.
          </p>
          <p className="mt-2 text-[12px] text-ink-3">Por enquanto o snapshot existe só neste Mac. Enviá-lo a um remote é um passo à parte, com confirmação.</p>
          {result.warnings.includes("embeddedRepository") && (
            <p className="mt-2 text-[12px] text-review">Há um repositório dentro deste: ele foi registrado só como referência, sem o conteúdo dele.</p>
          )}
        </div>
        <div className="flex justify-end gap-2 border-t border-line px-6 py-3">
          {repo.remotes.length > 0 && (
            <Button onClick={() => onPush(result.refName)}>
              Enviar para um remote…
            </Button>
          )}
          <Button variant="primary" onClick={onClose}>
            OK
          </Button>
        </div>
      </Modal>
    );
  }

  return (
    <Modal onClose={busy ? undefined : onClose} width={680}>
      <div className="border-b border-line px-6 pt-5 pb-4">
        <h2 className="text-[16px] font-semibold">Snapshot do trabalho em andamento</h2>
        <p className="mt-1 text-ink-2">
          Salva o estado atual de <span className="font-medium text-ink">{repo.path.split("/").pop()}</span> num branch novo. Não altera arquivos, o branch atual nem o
          índice: só cria objetos novos no <span className="font-mono">.git</span> e um branch local.
        </p>
      </div>
      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-4">
        {query.isError && <ErrorNote>{errorMessage(query.error)}</ErrorNote>}
        {!plan && !query.isError && <div className="text-ink-3">Calculando o que entra no snapshot…</div>}
        {plan && (
          <>
            <dl className="grid grid-cols-[130px_1fr] gap-x-4 gap-y-1.5 text-[12.5px]">
              <dt className="text-ink-3">Branch atual</dt>
              <dd className="font-mono">{plan.branch ?? "(HEAD solto)"} <span className="font-sans text-ink-3">· não será alterado</span></dd>
              <dt className="text-ink-3">Será criado</dt>
              <dd className="font-mono selectable">{plan.refName || "—"}</dd>
              <dt className="text-ink-3">Arquivos alterados</dt>
              <dd>{formatCount(plan.changed)} já versionados, salvos como estão no disco</dd>
              <dt className="text-ink-3">Arquivos novos</dt>
              <dd>
                {formatCount(plan.untrackedCount)} · {formatBytes(plan.untrackedBytes)}
              </dd>
            </dl>

            {plan.untracked.length > 0 && (
              <div>
                <div className="mb-1 text-[12px] font-medium text-ink-2">Maiores arquivos novos</div>
                <ul className="divide-y divide-line rounded-lg border border-line">
                  {plan.untracked.slice(0, 8).map((f) => (
                    <li key={f.path} className="flex items-center gap-3 px-3 py-1.5 text-[12px]">
                      <span className="min-w-0 flex-1 truncate font-mono selectable" title={f.path}>
                        {f.path}
                      </span>
                      <span className="tabular text-ink-2">{formatBytes(f.size)}</span>
                    </li>
                  ))}
                </ul>
                {plan.untrackedCount > 8 && <div className="mt-1 text-[11.5px] text-ink-3">e mais {formatCount(plan.untrackedCount - 8)} arquivos novos</div>}
              </div>
            )}

            {decisions.length > 0 && (
              <div>
                <div className="mb-1 flex items-center gap-1.5 text-[12px] font-medium text-ink-2">
                  <ShieldAlert className="size-3.5 text-review" /> Precisam de uma decisão
                </div>
                <p className="mb-1.5 text-[11.5px] text-ink-3">
                  Só o caminho e o tipo aparecem aqui; o conteúdo nunca é mostrado. O que ficar de fora não entra no snapshot (arquivos já versionados mantêm a versão do
                  último commit).
                </p>
                <ul className="divide-y divide-line rounded-lg border border-line">
                  {decisions.map(([path, why]) => (
                    <li key={path}>
                      <label className="flex cursor-pointer items-center gap-3 px-3 py-2 text-[12px]">
                        <input type="checkbox" className="accent-[var(--accent)]" checked={excludes.includes(path)} onChange={() => toggle(path)} />
                        <span className="min-w-0 flex-1 truncate font-mono selectable" title={shortPath(`${repo.path}/${path}`, home)}>
                          {path}
                        </span>
                        <span className="text-ink-2">{why.join(", ")}</span>
                        <Badge tone={excludes.includes(path) ? "safe" : "review"}>{excludes.includes(path) ? "fica de fora" : "entra"}</Badge>
                      </label>
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {plan.trackedSecrets > 0 && (
              <p className="flex gap-1.5 text-[12px] text-ink-3">
                <Info className="mt-px size-3.5 shrink-0" />
                {plan.trackedSecrets} {plan.trackedSecrets === 1 ? "arquivo com aparência de segredo já está" : "arquivos com aparência de segredo já estão"} no repositório. Isso não
                bloqueia o snapshot, mas vale revisar antes de enviar a algum lugar.
              </p>
            )}

            {plan.blockers.length > 0 && (
              <ul className="space-y-1 rounded-lg border border-review/30 bg-[color-mix(in_srgb,var(--review)_10%,transparent)] px-3 py-2">
                {plan.blockers.map((b, i) => (
                  <li key={i} className="text-[12px] text-review">
                    {blockerText(b)}
                  </li>
                ))}
              </ul>
            )}
          </>
        )}
        {error && <ErrorNote>{error}</ErrorNote>}
      </div>
      <div className="flex justify-end gap-2 border-t border-line px-6 py-3">
        <Button variant="ghost" onClick={onClose} disabled={busy}>
          Cancelar
        </Button>
        <Button variant="primary" busy={busy} disabled={blocked} onClick={create}>
          Criar snapshot local
        </Button>
      </div>
    </Modal>
  );
}
