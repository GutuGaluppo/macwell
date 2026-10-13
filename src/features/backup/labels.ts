// pt-BR wording for the neutral codes the Rust side reports.
import type { BackupSource, TimeMachineState, Unreadable, UnreadableReason } from "../../types";
import { formatAge, formatDateTime } from "../../lib/format";

export const sourceLabel: Record<BackupSource, string> = {
  preferences: "as preferências do Time Machine",
  destinationInfo: "os destinos do Time Machine",
  status: "o estado atual do Time Machine",
  latestBackup: "o último backup no disco",
  localSnapshots: "os instantâneos locais",
  mobileMeAccounts: "as contas do iCloud",
  finderPreferences: "Mesa e Documentos no iCloud",
};

export const reasonLabel: Record<UnreadableReason, string> = {
  permissionDenied: "o macOS recusou a leitura (em geral falta o Acesso Total ao Disco)",
  notMounted: "o disco de backup não está conectado",
  unavailable: "a leitura falhou ou veio num formato desconhecido",
};

export function unreadableText(u: Unreadable): string {
  return `Não consegui ler ${sourceLabel[u.source]}: ${reasonLabel[u.reason]}.`;
}

export function statusHeadline(tm: TimeMachineState, staleDays: number): string {
  switch (tm.status) {
    case "noDestination":
      return "O Time Machine não tem nenhum destino configurado: nada está sendo copiado por ele.";
    case "neverBackedUp":
      return "Há um destino do Time Machine, mas nenhum backup concluído é conhecido.";
    case "stale":
      return `O último backup concluído foi ${formatAge(tm.lastBackupMs)} (${formatDateTime(tm.lastBackupMs)}), acima do limite de ${staleDays} dias.`;
    case "recent":
      return `Último backup concluído ${formatAge(tm.lastBackupMs)} (${formatDateTime(tm.lastBackupMs)}).`;
    case "unknown":
      return "Não foi possível verificar o estado do Time Machine.";
  }
}

export const repoReasonLabel: Record<string, string> = {
  uncommitted: "alterações sem commit",
  unpushedCommits: "commits não enviados",
  noUpstream: "branch sem upstream",
  noRemote: "sem remote",
};

export const statusTone = { recent: "safe", stale: "danger", noDestination: "danger", neverBackedUp: "danger", unknown: "review" } as const;
export const statusBadge: Record<TimeMachineState["status"], string> = {
  recent: "Recente",
  stale: "Antigo",
  noDestination: "Sem destino",
  neverBackedUp: "Nunca concluído",
  unknown: "Desconhecido",
};
