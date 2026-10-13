import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, Globe, ShieldAlert } from "lucide-react";
import { Badge, Button, ErrorNote } from "../../components/ui";
import { Modal } from "../../components/ReviewDialog";
import { api, errorMessage } from "../../lib/ipc";
import { formatDateTime } from "../../lib/format";
import type { PushPlan, PushResult, RepoReport } from "../../types";
import { remoteLabel, visibilityLabel } from "./labels";

/**
 * Sending a snapshot branch is a separate, confirmed action: one branch, one configured remote, no
 * force. When the remote is public (or nobody checked), the warning has to be accepted explicitly.
 */
export function PushDialog({ repo, refName, onClose, onPushed }: { repo: RepoReport; refName: string; onClose: () => void; onPushed: () => void }) {
  const qc = useQueryClient();
  const [remote, setRemote] = useState(repo.remotes.find((r) => r.name === "origin")?.name ?? repo.remotes[0]?.name ?? "");
  const [accept, setAccept] = useState(false);
  const [checking, setChecking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<PushResult | null>(null);

  const key = ["pushPlan", repo.path, refName, remote];
  const query = useQuery({ queryKey: key, queryFn: () => api.planPush(repo.path, refName, remote), enabled: !!remote, gcTime: 0, retry: false });
  const plan = query.data;

  useEffect(() => setAccept(false), [remote, plan?.warning]);

  const verify = async () => {
    setChecking(true);
    setError(null);
    try {
      qc.setQueryData<PushPlan>(key, await api.checkRemoteVisibility(repo.path, remote, refName));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setChecking(false);
    }
  };

  const send = async () => {
    setBusy(true);
    setError(null);
    try {
      const res = await api.pushSnapshot(repo.path, refName, remote, true, accept);
      setDone(res);
      onPushed();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  if (done) {
    return (
      <Modal onClose={onClose}>
        <div className="p-6">
          <div className="flex items-center gap-2 text-[16px] font-semibold">
            <CheckCircle2 className="size-5 text-safe" /> Branch enviado
          </div>
          <p className="mt-2 text-ink-2">
            <span className="font-mono text-ink">{done.refName}</span> foi enviado para <span className="font-medium text-ink">{done.remote}</span>. Nada foi sobrescrito e o
            seu branch atual continua como estava.
          </p>
        </div>
        <div className="flex justify-end border-t border-line px-6 py-3">
          <Button variant="primary" onClick={onClose}>
            OK
          </Button>
        </div>
      </Modal>
    );
  }

  const github = plan?.kind.kind === "github";
  return (
    <Modal onClose={busy ? undefined : onClose} width={620}>
      <div className="border-b border-line px-6 pt-5 pb-4">
        <h2 className="text-[16px] font-semibold">Enviar snapshot para um remote</h2>
        <p className="mt-1 text-ink-2">
          Envia só o branch <span className="font-mono text-ink selectable">{refName}</span>, sem forçar e sem tocar em outros branches.
        </p>
      </div>
      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-4">
        {repo.remotes.length > 1 && (
          <label className="flex items-center gap-3 text-[12.5px]">
            <span className="text-ink-3">Remote</span>
            <select className="glass h-8 rounded-lg px-2 text-[13px]" value={remote} onChange={(e) => setRemote(e.target.value)}>
              {repo.remotes.map((r) => (
                <option key={r.name} value={r.name}>
                  {r.name}
                </option>
              ))}
            </select>
          </label>
        )}
        {query.isError && <ErrorNote>{errorMessage(query.error)}</ErrorNote>}
        {plan && (
          <>
            <dl className="grid grid-cols-[120px_1fr] gap-x-4 gap-y-1.5 text-[12.5px]">
              <dt className="text-ink-3">Destino</dt>
              <dd className="break-all font-mono selectable">{plan.url}</dd>
              <dt className="text-ink-3">Tipo</dt>
              <dd>{remoteLabel(plan.kind)}</dd>
              <dt className="text-ink-3">Visibilidade</dt>
              <dd className="flex flex-wrap items-center gap-2">
                {plan.visibility ? (
                  <>
                    <Badge tone={plan.visibility === "public" ? "danger" : plan.visibility === "privateOrMissing" ? "safe" : "review"}>{visibilityLabel[plan.visibility]}</Badge>
                    <span className="text-[11.5px] text-ink-3">verificado em {formatDateTime(plan.visibilityCheckedAt)}</span>
                  </>
                ) : (
                  <span className="text-ink-2">{plan.kind.kind === "local" ? "Não se aplica: fica neste Mac." : "Ainda não verificada"}</span>
                )}
                {github && (
                  <Button size="sm" icon={<Globe className="size-3.5" />} busy={checking} onClick={verify}>
                    {plan.visibility ? "Verificar de novo" : "Verificar agora"}
                  </Button>
                )}
              </dd>
            </dl>
            {github && (
              <p className="text-[11.5px] text-ink-3">
                “Verificar” faz uma consulta anônima a api.github.com com o nome do repositório e nada mais: sem login e sem arquivos. Só acontece quando você clica.
              </p>
            )}
            {plan.warning && (
              <div className="rounded-lg border border-danger/30 bg-danger-soft px-3 py-2.5">
                <div className="flex items-center gap-1.5 text-[12.5px] font-medium text-danger">
                  <ShieldAlert className="size-4" /> {plan.warning === "public" ? "Este remote é público" : "Não sei se este remote é público"}
                </div>
                <p className="mt-1 text-[12px] text-ink-2">
                  {plan.warning === "public"
                    ? "Qualquer pessoa poderá ler tudo o que está no snapshot, inclusive arquivos que você nunca tinha commitado."
                    : "Sem verificação, trato como se fosse público: qualquer pessoa poderia ler o conteúdo do snapshot, inclusive arquivos que você nunca tinha commitado."}
                </p>
                <label className="mt-2 flex cursor-pointer items-start gap-2 text-[12.5px] text-ink">
                  <input type="checkbox" className="mt-0.5 accent-[var(--accent)]" checked={accept} onChange={(e) => setAccept(e.target.checked)} />
                  Entendo e quero enviar mesmo assim
                </label>
              </div>
            )}
          </>
        )}
        {error && <ErrorNote>{error}</ErrorNote>}
      </div>
      <div className="flex justify-end gap-2 border-t border-line px-6 py-3">
        <Button variant="ghost" onClick={onClose} disabled={busy}>
          Cancelar
        </Button>
        <Button variant={plan?.warning ? "danger" : "primary"} busy={busy} disabled={!plan || (!!plan.warning && !accept)} onClick={send}>
          Enviar branch
        </Button>
      </div>
    </Modal>
  );
}
