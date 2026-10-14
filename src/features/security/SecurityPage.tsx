import { useState } from "react";
import { ChevronLeft, FileWarning, FolderOpen, Info, LogIn, Rocket, ShieldAlert } from "lucide-react";
import { api } from "../../lib/ipc";
import { useJob, useJobs } from "../../stores/jobs";
import { Badge, Button, Card, ErrorNote, PageHeader } from "../../components/ui";
import { ModuleStage } from "../../components/ModuleStage";
import { BentoButton, BentoIcon, ResultsBento } from "../../components/ResultsBento";
import securityHero from "../../assets/icons/seguranca-hd.webp";
import { useHome } from "../scanner/ScannerPage";
import { formatCount, shortPath } from "../../lib/format";
import { AppIcon } from "../applications/shared";
import type { Finding, SecurityAudit, SignatureKind } from "../../types";

type Tab = "persistence" | "apps";

const MODULE = "security";

const signatureLabel: Record<SignatureKind, string> = {
  apple: "Apple",
  appStore: "App Store",
  developerId: "Developer ID",
  development: "Certificado de desenvolvimento",
  adHoc: "Assinatura local (ad hoc)",
  unsigned: "Sem assinatura",
  invalid: "Assinatura não confere",
  unknown: "Desconhecida",
};

/** Plain explanations, no alarm: what was seen and why it can matter. */
const findingText: Record<string, string> = {
  unsigned: "O programa não tem assinatura de código: não dá para saber quem o publicou.",
  adhoc: "Assinado só localmente (ad hoc), sem identidade de desenvolvedor. Comum em ferramentas compiladas no próprio Mac.",
  invalidSignature: "A assinatura não confere com os arquivos: o app foi modificado depois de assinado. Comum em builds de desenvolvimento.",
  developmentSigned: "Assinado com certificado de desenvolvimento, como builds feitos no Xcode deste Mac.",
  gatekeeperRejected: "O Gatekeeper não aprovaria abrir este app pela primeira vez (não é notarizado nem da App Store).",
  applePrefixNotApple: "O nome começa com com.apple., mas o programa não é assinado pela Apple. Vale conferir de onde veio.",
  temporaryLocation: "O programa fica numa pasta temporária ou compartilhada, um lugar incomum para itens de início.",
  hiddenLocation: "O programa fica numa pasta oculta.",
  inlineScript: "Executa um script escrito direto na configuração (ex.: bash -c).",
  programMissing: "O programa não existe mais: é um item de início que sobrou de um app removido.",
  noProgram: "A configuração não indica um programa para executar.",
  disabled: "Está desativado na própria configuração.",
};

const scopeLabel = { user: "Só você", allUsers: "Todos os usuários", system: "Sistema (root)" };

function Findings({ findings }: { findings: Finding[] }) {
  if (!findings.length) return <span className="text-[11.5px] text-ink-3">Nada a observar</span>;
  return (
    <ul className="space-y-0.5">
      {findings.map((f) => (
        <li key={f.code} className={`text-[11.5px] ${f.level === "attention" ? "text-review" : "text-ink-2"}`}>
          {findingText[f.code] ?? f.code}
        </li>
      ))}
    </ul>
  );
}

function AuditDetails({ initialTab, onBack }: { initialTab: Tab; onBack: () => void }) {
  const home = useHome();
  const job = useJob<SecurityAudit>(MODULE);
  const [tab, setTab] = useState<Tab>(initialTab);
  const [onlyNotes, setOnlyNotes] = useState(true);
  const r = job.result;
  const items = (r?.persistence ?? []).filter((i) => !onlyNotes || i.findings.length);
  const apps = (r?.apps ?? []).filter((a) => !onlyNotes || a.findings.length);

  return (
    <div className="h-full overflow-y-auto">
      <PageHeader
        title="Auditoria de segurança"
        subtitle="O que inicia sozinho no Mac e como os apps estão assinados. É uma auditoria experimental, não um antivírus: não detecta malware e não muda nada no sistema."
        actions={
          <Button variant="ghost" icon={<ChevronLeft className="size-3.5" />} onClick={onBack}>
            Voltar aos resultados
          </Button>
        }
      />
      <div className="space-y-4 px-8 pb-10">
        {r && (
          <>
            <div className="flex flex-wrap items-center gap-3">
              <div className="inline-flex rounded-lg glass p-0.5">
                {(["persistence", "apps"] as const).map((t) => (
                  <button key={t} onClick={() => setTab(t)} className={`h-7 rounded-md px-3 text-[12px] ${tab === t ? "bg-accent font-medium text-accent-ink" : "text-ink-2 hover:text-ink"}`}>
                    {t === "persistence" ? `Itens de início (${r.persistence.length})` : `Apps (${r.apps.length})`}
                  </button>
                ))}
              </div>
              <label className="flex items-center gap-2 text-[12.5px] text-ink-2">
                <input type="checkbox" className="accent-[var(--accent)]" checked={onlyNotes} onChange={(e) => setOnlyNotes(e.target.checked)} />
                Só os que têm observações
              </label>
              <div className="flex-1" />
              <Button size="sm" onClick={() => api.openSystemSettings("loginItems")}>
                Abrir Itens de Início
              </Button>
            </div>
            <p className="flex gap-1.5 text-[12px] text-ink-3">
              <Info className="mt-px size-3.5 shrink-0" /> Para desativar um item, use Ajustes do Sistema → Geral → Itens de Início, ou desinstale o app que o
              instalou. Observações não significam que algo seja malicioso.
            </p>
            {tab === "persistence" && (
              <Card className="divide-y divide-line">
                {items.length === 0 && <div className="px-4 py-6 text-center text-ink-3">Nenhuma observação.</div>}
                {items.map((i) => (
                  <div key={i.plist} className="grid grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)_28px] gap-4 px-4 py-2.5">
                    <div className="min-w-0">
                      <div className="flex items-center gap-1.5">
                        <span className="truncate font-medium">{i.label || i.plist.split("/").pop()}</span>
                        <Badge>{scopeLabel[i.scope]}</Badge>
                        {i.runAtLoad && <Badge>ao iniciar</Badge>}
                      </div>
                      <div className="truncate font-mono text-[11px] text-ink-3 selectable" title={[i.program, ...i.arguments].join(" ")}>
                        {i.program ? shortPath(i.program, home) : "—"} {i.arguments.join(" ")}
                      </div>
                      <div className="text-[11px] text-ink-3">{i.signature ? `${signatureLabel[i.signature.kind]}${i.signature.teamId ? ` · ${i.signature.teamId}` : ""}` : ""}</div>
                    </div>
                    <Findings findings={i.findings} />
                    <button className="text-ink-3 hover:text-ink" title="Mostrar a configuração no Finder" onClick={() => api.revealInFinder(i.plist)}>
                      <FolderOpen className="size-4" />
                    </button>
                  </div>
                ))}
              </Card>
            )}
            {tab === "apps" && (
              <Card className="divide-y divide-line">
                {apps.length === 0 && <div className="px-4 py-6 text-center text-ink-3">Nenhuma observação.</div>}
                {apps.map((a) => (
                  <div key={a.path} className="grid grid-cols-[36px_minmax(0,1fr)_minmax(0,1.2fr)_28px] items-center gap-3 px-4 py-2">
                    <AppIcon path={a.path} />
                    <div className="min-w-0">
                      <div className="truncate font-medium">{a.name}</div>
                      <div className="truncate text-[11.5px] text-ink-3">
                        {signatureLabel[a.signature.kind]}
                        {a.signature.teamId ? ` · ${a.signature.teamId}` : ""}
                        {a.gatekeeperChecked && a.gatekeeper ? ` · ${a.gatekeeper}` : ""}
                        {a.quarantined ? " · baixado da internet" : ""}
                      </div>
                    </div>
                    <Findings findings={a.findings} />
                    <button className="text-ink-3 hover:text-ink" title="Mostrar no Finder" onClick={() => api.revealInFinder(a.path)}>
                      <FolderOpen className="size-4" />
                    </button>
                  </div>
                ))}
              </Card>
            )}
          </>
        )}
      </div>
    </div>
  );
}

const attention = (f: Finding[]) => f.some((x) => x.level === "attention");

// Intro, scan and results around the audit. Nothing is read until the user presses Auditar.
export function SecurityPage() {
  const job = useJob<SecurityAudit>(MODULE);
  const { start, cancel, reset } = useJobs();
  const [view, setView] = useState<Tab | null>(null);
  const r = job.result;
  const phase = job.status === "running" ? "scanning" : job.status === "done" || job.status === "failed" ? "results" : "intro";

  const persistence = r?.persistence ?? [];
  const apps = r?.apps ?? [];
  const notedItems = persistence.filter((i) => i.findings.length);
  const notedApps = apps.filter((a) => a.findings.length);
  const urgent = persistence.filter((i) => attention(i.findings)).length + apps.filter((a) => attention(a.findings)).length;
  const unsigned = apps.filter((a) => a.signature.kind === "unsigned" || a.signature.kind === "invalid").length;
  const byKind = Object.entries(
    apps.reduce<Record<string, number>>((acc, a) => {
      acc[a.signature.kind] = (acc[a.signature.kind] ?? 0) + 1;
      return acc;
    }, {}),
  ).sort((x, y) => y[1] - x[1]);
  const scan = () => start(MODULE, "start_security_audit", {});

  return (
    <ModuleStage
      phase={phase}
      title="Segurança"
      hero={securityHero}
      intro="Veja o que inicia sozinho no Mac e como cada app está assinado. É uma auditoria experimental, não um antivírus: não detecta malware e não muda nada no sistema."
      scanLabel="Auditar"
      progress={job.stage?.stage === "apps" ? job.stage.done / Math.max(job.stage.total, 1) : undefined}
      scanning="Verificando o sistema…"
      detail={job.stage?.stage === "apps" ? `Verificando assinaturas (${formatCount(job.stage.done)} de ${formatCount(job.stage.total)})` : "Lendo itens de início"}
      onScan={scan}
      onStop={() => {
        cancel(MODULE);
        reset(MODULE);
      }}
      onBack={() => {
        setView(null);
        reset(MODULE);
      }}
    >
      {view ? (
        <AuditDetails initialTab={view} onBack={() => setView(null)} />
      ) : (
        <>
          {job.status === "failed" && (
            <div className="px-8">
              <ErrorNote>{job.error}</ErrorNote>
            </div>
          )}
          {r && (
            <ResultsBento
              headline={urgent ? `${formatCount(urgent)} ${urgent === 1 ? "item merece" : "itens merecem"} atenção.` : "Nenhum item pede atenção."}
              pill={<BentoButton onClick={() => setView("persistence")}>Ver todos os itens</BentoButton>}
              extra={
                <BentoButton onClick={scan}>Auditar de novo</BentoButton>
              }
              tiles={[
                {
                  id: "persistence",
                  title: `${formatCount(notedItems.length)} de ${formatCount(persistence.length)} itens de início com observações`,
                  description: "Programas que iniciam sozinhos com o Mac ou com a sua conta. Observação não significa que algo seja malicioso.",
                  visual: <BentoIcon size={112}><Rocket /></BentoIcon>,
                  actions: (
                    <BentoButton primary onClick={() => setView("persistence")}>
                      Revisar
                    </BentoButton>
                  ),
                },
                {
                  id: "apps",
                  title: `${formatCount(notedApps.length)} de ${formatCount(apps.length)} apps com observações`,
                  description: "Assinatura de código e aprovação do Gatekeeper de cada app em /Applications.",
                  visual: <BentoIcon size={56}><ShieldAlert /></BentoIcon>,
                  body: (
                    <div className="flex flex-wrap gap-1.5">
                      {byKind.map(([kind, n]) => (
                        <Badge key={kind}>
                          {signatureLabel[kind as SignatureKind]} · {formatCount(n)}
                        </Badge>
                      ))}
                    </div>
                  ),
                  actions: (
                    <BentoButton primary onClick={() => setView("apps")}>
                      Revisar
                    </BentoButton>
                  ),
                },
                {
                  id: "unsigned",
                  title: `${formatCount(unsigned)} ${unsigned === 1 ? "app sem" : "apps sem"} assinatura válida`,
                  description: "Sem assinatura, ou com assinatura que não confere com os arquivos.",
                  visual: <BentoIcon size={48}><FileWarning /></BentoIcon>,
                  actions: (
                    <BentoButton primary disabled={!unsigned} onClick={() => setView("apps")}>
                      Revisar
                    </BentoButton>
                  ),
                },
                {
                  id: "loginItems",
                  title: "Itens de Início do sistema",
                  description: "Para desativar um item, use Ajustes do Sistema ou desinstale o app que o instalou.",
                  visual: <BentoIcon size={48}><LogIn /></BentoIcon>,
                  actions: <BentoButton primary onClick={() => api.openSystemSettings("loginItems")}>Abrir</BentoButton>,
                },
              ]}
            />
          )}
        </>
      )}
    </ModuleStage>
  );
}
