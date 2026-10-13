import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Cloud, HardDrive, RefreshCw, ShieldCheck } from "lucide-react";
import { api, errorMessage } from "../../lib/ipc";
import { Badge, Button, Card, ErrorNote, PageHeader } from "../../components/ui";
import { useNav } from "../../stores/nav";
import { useHome } from "../scanner/ScannerPage";
import { formatBytes, formatDateTime, shortPath } from "../../lib/format";
import { ExclusionsCard } from "./ExclusionsCard";
import { repoReasonLabel, statusBadge, statusHeadline, statusTone, unreadableText } from "./labels";

const LIMITS = [3, 7, 14, 30];

const yesNo = (v: boolean | null, yes: string, no: string, unknown = "Não foi possível saber") => (v === null ? unknown : v ? yes : no);

export function BackupPage() {
  const home = useHome();
  const qc = useQueryClient();
  const go = useNav((s) => s.go);
  const { data: gate, error, isFetching, refetch } = useQuery({ queryKey: ["backupGate"], queryFn: api.getBackupGate, staleTime: 0 });
  const tm = gate?.backup.timeMachine;
  const cloud = gate?.backup.icloud;

  const setLimit = async (days: number) => {
    await api.setBackupStaleDays(days);
    qc.invalidateQueries({ queryKey: ["backupGate"] });
  };

  return (
    <div className="h-full overflow-y-auto">
      <PageHeader
        title="Backup"
        subtitle="O que o Mac sabe sobre os próprios backups. Só leitura: nada aqui altera o Time Machine nem o iCloud. Antes de uma limpeza em lote, o app confere isto de novo."
        actions={
          <Button icon={<RefreshCw className="size-3.5" />} busy={isFetching} onClick={() => refetch()}>
            Reler
          </Button>
        }
      />
      <div className="space-y-4 px-8 pb-10">
        {error && <ErrorNote>{errorMessage(error)}</ErrorNote>}
        {!gate && !error && <div className="text-ink-3">Lendo o estado do backup…</div>}
        {gate && tm && cloud && (
          <>
            <Card className="p-5">
              <div className="mb-3 flex items-center gap-2">
                <ShieldCheck className="size-4 text-accent" />
                <span className="text-[13px] font-medium">Antes de limpar em lote</span>
                <Badge tone={gate.level === "ok" ? "safe" : gate.level === "warn" ? "review" : "danger"}>
                  {gate.level === "ok" ? "Livre" : gate.level === "warn" ? "Pede um aviso" : "Pede confirmação reforçada"}
                </Badge>
              </div>
              <p className="text-[12.5px] text-ink-2">
                {gate.level === "ok" && "Há um backup recente e nenhum repositório com trabalho só neste Mac. Nenhuma pergunta extra."}
                {gate.level === "warn" && "Algo merece uma olhada: o app mostra o motivo e pede que você confirme que viu."}
                {gate.level === "strong" && "Sem backup recente, o app pede que você digite CONFIRMO antes de uma limpeza em lote."}
              </p>
              <p className="mt-2 text-[11.5px] text-ink-3">
                Vale para o Cuidado inteligente, as regras de Limpeza avançada, restos de apps, duplicados e pastas regeneráveis. A conferência é feita pelo próprio app, não só pela tela.
              </p>
            </Card>

            <Card className="p-5">
              <div className="mb-3 flex flex-wrap items-center gap-2">
                <HardDrive className="size-4 text-accent" />
                <span className="text-[13px] font-medium">Time Machine</span>
                <Badge tone={statusTone[tm.status]}>{statusBadge[tm.status]}</Badge>
                <div className="flex-1" />
                <label className="flex items-center gap-2 text-[12px] text-ink-2">
                  Considerar antigo após
                  <select className="glass h-7 rounded-md px-1.5 text-[12px]" value={gate.backup.staleDays} onChange={(e) => setLimit(Number(e.target.value))}>
                    {(LIMITS.includes(gate.backup.staleDays) ? LIMITS : [...LIMITS, gate.backup.staleDays].sort((a, b) => a - b)).map((d) => (
                      <option key={d} value={d}>
                        {d} dias
                      </option>
                    ))}
                  </select>
                </label>
              </div>
              <p className="text-[12.5px] text-ink">{statusHeadline(tm, gate.backup.staleDays)}</p>
              <dl className="mt-3 grid grid-cols-[200px_1fr] gap-x-4 gap-y-1.5 text-[12.5px]">
                <dt className="text-ink-3">Backups automáticos</dt>
                <dd className={tm.autoBackup === false ? "text-review" : ""}>{yesNo(tm.autoBackup, "Ligados", "Desligados")}</dd>
                <dt className="text-ink-3">Última tentativa</dt>
                <dd>{formatDateTime(tm.lastAttemptMs)}</dd>
                <dt className="text-ink-3">Em andamento agora</dt>
                <dd>{tm.running ? `Sim${tm.percent !== null ? ` (${Math.round(tm.percent * 100)}%)` : ""}` : "Não"}</dd>
                <dt className="text-ink-3">Instantâneos locais</dt>
                <dd>
                  {tm.localSnapshotCount}
                  <span className="text-ink-3"> · ficam no próprio disco do Mac; não são um backup.</span>
                </dd>
              </dl>
              {tm.destinations.length > 0 && (
                <ul className="mt-3 divide-y divide-line rounded-lg border border-line">
                  {tm.destinations.map((d) => (
                    <li key={d.id} className="flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-2 text-[12.5px]">
                      <span className="font-medium">{d.name ?? "Destino"}</span>
                      <Badge tone={d.mounted === false ? "review" : "neutral"}>{d.mounted === null ? "conexão desconhecida" : d.mounted ? "conectado" : "desconectado"}</Badge>
                      {d.encrypted !== null && <span className="text-ink-3">{d.encrypted ? "criptografado" : "sem criptografia"}</span>}
                      {d.bytesUsed !== null && d.quotaGb !== null && (
                        <span className="text-ink-3">
                          {formatBytes(d.bytesUsed)} de {d.quotaGb} GB
                        </span>
                      )}
                      <span className="ml-auto text-[11.5px] text-ink-3">último backup: {formatDateTime(d.lastBackupMs)}</span>
                    </li>
                  ))}
                </ul>
              )}
              {tm.unreadable.length > 0 && (
                <ul className="mt-3 space-y-0.5 rounded-lg bg-[color-mix(in_srgb,var(--review)_10%,transparent)] px-3 py-2 text-[11.5px] text-review">
                  {tm.unreadable.map((u, i) => (
                    <li key={i}>{unreadableText(u)}</li>
                  ))}
                </ul>
              )}
            </Card>

            <Card className="p-5">
              <div className="mb-3 flex items-center gap-2">
                <Cloud className="size-4 text-accent" />
                <span className="text-[13px] font-medium">iCloud Drive</span>
                <Badge>inferido</Badge>
              </div>
              <dl className="grid grid-cols-[200px_1fr] gap-x-4 gap-y-1.5 text-[12.5px]">
                <dt className="text-ink-3">Conta do iCloud neste Mac</dt>
                <dd>{yesNo(cloud.signedIn, "Conectada", "Não conectada")}</dd>
                <dt className="text-ink-3">iCloud Drive</dt>
                <dd>{yesNo(cloud.driveEnabled, "Ligado", "Desligado")}</dd>
                <dt className="text-ink-3">Mesa e Documentos</dt>
                <dd>
                  {cloud.desktopSynced === null || cloud.documentsSynced === null
                    ? "Não foi possível saber"
                    : cloud.desktopSynced && cloud.documentsSynced
                      ? "Sincronizados"
                      : cloud.desktopSynced || cloud.documentsSynced
                        ? "Só um deles sincroniza"
                        : "Não sincronizam"}
                </dd>
              </dl>
              <p className="mt-2 text-[11.5px] text-ink-3">
                Sinais locais, sem consultar a Apple e sem ler dados da conta. O iCloud espelha o que você apaga: ele ajuda a ter os arquivos em outro lugar, mas não substitui um backup.
              </p>
              {cloud.unreadable.length > 0 && (
                <ul className="mt-2 space-y-0.5 text-[11.5px] text-review">
                  {cloud.unreadable.map((u, i) => (
                    <li key={i}>{unreadableText(u)}</li>
                  ))}
                </ul>
              )}
            </Card>

            <ExclusionsCard />

            <Card className="p-5">
              <div className="mb-2 text-[13px] font-medium">Trabalho que só existe neste Mac</div>
              {gate.unpushed.length === 0 ? (
                <p className="text-[12.5px] text-ink-2">
                  {gate.unreadableRepos > 0 ? `Nenhum encontrado, mas não consegui ler ${gate.unreadableRepos} repositório(s).` : "Nenhum repositório com trabalho pendente nas pastas de projetos."}
                </p>
              ) : (
                <>
                  <ul className="divide-y divide-line rounded-lg border border-line">
                    {gate.unpushed.map((r) => (
                      <li key={r.path} className="flex items-center gap-3 px-3 py-2 text-[12.5px]">
                        <span className="min-w-0 flex-1 truncate font-mono text-[11.5px]" title={r.path}>
                          {shortPath(r.path, home)}
                        </span>
                        <span className="text-[11.5px] text-ink-3">{r.reasons.map((x) => repoReasonLabel[x] ?? x).join(", ")}</span>
                      </li>
                    ))}
                  </ul>
                  <Button size="sm" className="mt-3" onClick={() => go("projects")}>
                    Abrir Projetos
                  </Button>
                </>
              )}
            </Card>
          </>
        )}
      </div>
    </div>
  );
}
