import { useState } from "react";
import { AlertTriangle, ShieldAlert } from "lucide-react";
import { Button } from "../../components/ui";
import { Modal } from "../../components/ReviewDialog";
import { api } from "../../lib/ipc";
import { useNav } from "../../stores/nav";
import { useBackupGateDialog } from "../../stores/backupGate";
import { shortPath } from "../../lib/format";
import { useHome } from "../scanner/ScannerPage";
import type { BackupGate } from "../../types";
import { repoReasonLabel, statusHeadline, unreadableText } from "./labels";

const CONFIRM_WORD = "CONFIRMO";

/** Shown by `requestBackupAck` when the gate is not "ok". Mounted once, in App. */
export function BackupGateHost() {
  const pending = useBackupGateDialog((s) => s.pending);
  if (!pending) return null;
  return <GateDialog gate={pending.gate} onAnswer={pending.resolve} />;
}

function GateDialog({ gate, onAnswer }: { gate: BackupGate; onAnswer: (ack: { acknowledged: boolean; typed?: string } | null) => void }) {
  const home = useHome();
  const go = useNav((s) => s.go);
  const [typed, setTyped] = useState("");
  const strong = gate.level === "strong";
  const tm = gate.backup.timeMachine;
  const ready = !strong || typed.trim().toUpperCase() === CONFIRM_WORD;
  const unreadable = [...tm.unreadable, ...gate.backup.icloud.unreadable];
  const needsAccess = unreadable.some((u) => u.reason === "permissionDenied");
  const icloudMirrors = strong && gate.backup.icloud.signedIn && (gate.backup.icloud.desktopSynced || gate.backup.icloud.documentsSynced);

  return (
    <Modal onClose={() => onAnswer(null)} width={560}>
      <div className="border-b border-line px-6 pt-5 pb-4">
        <h2 className="flex items-center gap-2 text-[16px] font-semibold">
          {strong ? <ShieldAlert className="size-5 text-danger" /> : <AlertTriangle className="size-5 text-review" />}
          {strong ? "Sem backup recente" : "Antes de continuar"}
        </h2>
        <p className="mt-1 text-ink-2">{statusHeadline(tm, gate.backup.staleDays)}</p>
      </div>
      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto px-6 py-4 text-[12.5px]">
        {tm.autoBackup === false && <p className="text-review">Os backups automáticos estão desligados: um novo backup só acontece se alguém o disparar.</p>}
        {tm.destinations.some((d) => d.mounted === false) && tm.status !== "noDestination" && (
          <p className="text-ink-2">O disco de backup não está conectado agora.</p>
        )}
        {icloudMirrors && (
          <p className="text-ink-2">O iCloud sincroniza Mesa e Documentos, mas também espelha exclusões: não vale como backup.</p>
        )}

        {gate.unpushed.length > 0 && (
          <div>
            <div className="mb-1 font-medium text-ink">
              {gate.unpushed.length} {gate.unpushed.length === 1 ? "repositório tem" : "repositórios têm"} trabalho que só existe neste Mac
            </div>
            <ul className="divide-y divide-line rounded-lg border border-line">
              {gate.unpushed.slice(0, 5).map((r) => (
                <li key={r.path} className="flex items-center gap-3 px-3 py-1.5">
                  <span className="min-w-0 flex-1 truncate font-mono text-[11.5px]" title={r.path}>
                    {shortPath(r.path, home)}
                  </span>
                  <span className="text-[11.5px] text-ink-3">{r.reasons.map((x) => repoReasonLabel[x] ?? x).join(", ")}</span>
                </li>
              ))}
            </ul>
            {gate.unpushed.length > 5 && <div className="mt-1 text-[11.5px] text-ink-3">e mais {gate.unpushed.length - 5}</div>}
            <Button
              size="sm"
              variant="ghost"
              className="mt-1"
              onClick={() => {
                onAnswer(null);
                go("projects");
              }}
            >
              Abrir Projetos para fazer um snapshot
            </Button>
          </div>
        )}
        {gate.unreadableRepos > 0 && <p className="text-ink-2">Não consegui ler {gate.unreadableRepos} {gate.unreadableRepos === 1 ? "repositório" : "repositórios"}: não sei se têm trabalho pendente.</p>}

        {unreadable.length > 0 && (
          <div className="rounded-lg bg-[color-mix(in_srgb,var(--review)_10%,transparent)] px-3 py-2">
            <ul className="space-y-0.5 text-[11.5px] text-review">
              {unreadable.map((u, i) => (
                <li key={i}>{unreadableText(u)}</li>
              ))}
            </ul>
            {needsAccess && (
              <Button size="sm" className="mt-2" onClick={() => api.openSystemSettings("fullDiskAccess")}>
                Abrir Acesso Total ao Disco
              </Button>
            )}
          </div>
        )}

        {strong && (
          <label className="block">
            <span className="text-ink-2">
              Para continuar sem um backup recente, digite <span className="font-mono font-semibold text-ink">{CONFIRM_WORD}</span>:
            </span>
            <input
              autoFocus
              value={typed}
              onChange={(e) => setTyped(e.target.value)}
              className="glass mt-1 h-8 w-full rounded-lg px-3 font-mono text-[13px] outline-none focus:ring-2 focus:ring-[var(--accent)]"
              aria-label="Confirmação"
              spellCheck={false}
            />
          </label>
        )}
        <p className="text-[11.5px] text-ink-3">O que for removido vai para a Lixeira e pode ser recuperado pelo Finder até ela ser esvaziada.</p>
      </div>
      <div className="flex justify-end gap-2 border-t border-line px-6 py-3">
        <Button variant="ghost" onClick={() => onAnswer(null)}>
          Cancelar
        </Button>
        <Button variant={strong ? "danger" : "primary"} disabled={!ready} onClick={() => onAnswer({ acknowledged: true, typed: strong ? typed : undefined })}>
          {strong ? "Continuar mesmo assim" : "Continuar"}
        </Button>
      </div>
    </Modal>
  );
}
