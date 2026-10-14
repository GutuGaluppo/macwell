import { useMemo, useState } from "react";
import { AppWindow, ChevronLeft, RefreshCw, Search } from "lucide-react";
import { useJob, useJobs } from "../../stores/jobs";
import { useNav } from "../../stores/nav";
import { Badge, Button, Card, EmptyState, ErrorNote, PageHeader, ProgressBar } from "../../components/ui";
import { AppsLoader } from "../../components/loaders";
import { ModuleStage } from "../../components/ModuleStage";
import { BentoButton, ResultsBento } from "../../components/ResultsBento";
import appsHero from "../../assets/icons/aplicativos-hd.webp";
import { useHome } from "../scanner/ScannerPage";
import { formatAge, formatBytes, formatCount, shortPath } from "../../lib/format";
import { AppIcon } from "./shared";
import { UninstallDialog } from "./UninstallDialog";
import type { AppInfo, AppsResult, OrphansResult, UpdatesResult } from "../../types";

const MODULE = "apps";
const UNUSED_DAYS = 180;

type Sort = "size" | "name" | "lastUsed";
type Filter = "all" | "unused" | "appStore";

const isUnused = (a: AppInfo) => !a.systemApp && (a.lastUsedAt ?? 0) < Date.now() - UNUSED_DAYS * 86_400_000;

function AppsList({ initialFilter, onBack }: { initialFilter: Filter; onBack: () => void }) {
  const home = useHome();
  const job = useJob<AppsResult>(MODULE);
  const start = useJobs((s) => s.start);
  const update = useJobs((s) => s.update);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<Sort>("size");
  const [filter, setFilter] = useState<Filter>(initialFilter);
  const [target, setTarget] = useState<AppInfo | null>(null);
  const r = job.result;
  const running = job.status === "running";

  const apps = useMemo(() => {
    const q = query.trim().toLowerCase();
    const cutoff = Date.now() - UNUSED_DAYS * 86_400_000;
    const list = (r?.apps ?? []).filter((a) => {
      if (q && !a.name.toLowerCase().includes(q) && !(a.bundleId ?? "").toLowerCase().includes(q)) return false;
      if (filter === "unused") return !a.systemApp && (a.lastUsedAt ?? 0) < cutoff;
      if (filter === "appStore") return a.fromAppStore;
      return true;
    });
    return list.sort((a, b) =>
      sort === "size" ? b.size - a.size : sort === "name" ? a.name.localeCompare(b.name, "pt-BR") : (a.lastUsedAt ?? 0) - (b.lastUsedAt ?? 0),
    );
  }, [r, query, sort, filter]);

  const unusedCount = (r?.apps ?? []).filter((a) => !a.systemApp && (a.lastUsedAt ?? 0) < Date.now() - UNUSED_DAYS * 86_400_000).length;
  const tab = (id: Filter, label: string) => (
    <button
      onClick={() => setFilter(id)}
      className={`h-7 rounded-md px-2.5 text-[12px] ${filter === id ? "bg-accent font-medium text-accent-ink" : "text-ink-2 hover:text-ink"}`}
    >
      {label}
    </button>
  );

  return (
    <div className="h-full overflow-y-auto">
      <PageHeader
        title="Aplicativos"
        subtitle="Apps em /Applications e ~/Applications, com tamanho e último uso. Desinstalar leva junto os arquivos do app em ~/Library, com a prévia de cada um."
        actions={
          <>
            <Button variant="ghost" icon={<ChevronLeft className="size-3.5" />} onClick={onBack}>
              Voltar aos resultados
            </Button>
            <Button variant="ghost" icon={<RefreshCw className="size-3.5" />} busy={running} onClick={() => start(MODULE, "start_app_list", {})}>
              Atualizar
            </Button>
          </>
        }
      />
      <div className="space-y-4 px-8 pb-10">
        {job.status === "failed" && <ErrorNote>{job.error}</ErrorNote>}
        {running && !r && (
          <Card className="p-5">
            <AppsLoader />
            <div className="mb-3 text-[13px] font-medium">
              Medindo os apps {job.stage ? `(${formatCount(job.stage.done)} de ${formatCount(job.stage.total)})` : "…"}
            </div>
            <ProgressBar value={job.stage ? job.stage.done / Math.max(job.stage.total, 1) : undefined} indeterminate={!job.stage} />
          </Card>
        )}
        {r && (
          <>
            <div className="flex flex-wrap items-center gap-3">
              <div className="relative">
                <Search className="pointer-events-none absolute top-2 left-2.5 size-3.5 text-ink-3" />
                <input
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder="Buscar app"
                  className="h-8 w-56 rounded-lg glass pr-3 pl-8 text-[12.5px] outline-none focus:border-accent"
                />
              </div>
              <div className="inline-flex rounded-lg glass p-0.5">
                {tab("all", `Todos (${formatCount(r.apps.length)})`)}
                {tab("unused", `Sem uso há 6 meses (${formatCount(unusedCount)})`)}
                {tab("appStore", "App Store")}
              </div>
              <select
                value={sort}
                onChange={(e) => setSort(e.target.value as Sort)}
                className="h-8 rounded-lg glass px-2 text-[12.5px]"
              >
                <option value="size">Maior primeiro</option>
                <option value="name">Nome</option>
                <option value="lastUsed">Usado há mais tempo</option>
              </select>
              <div className="ml-auto text-[12.5px] text-ink-2">
                <span className="tabular font-semibold text-ink">{formatBytes(r.totalSize)}</span> em apps
              </div>
            </div>
            {apps.length === 0 ? (
              <EmptyState icon={<AppWindow className="size-6" />} title="Nenhum app com esse filtro" />
            ) : (
              <Card className="divide-y divide-line overflow-hidden">
                {apps.map((a) => (
                  <div key={a.path} className="group grid grid-cols-[36px_minmax(0,1fr)_110px_90px_auto] items-center gap-3 px-4 py-2 hover:bg-surface-2">
                    <AppIcon path={a.path} />
                    <div className="min-w-0">
                      <div className="flex items-center gap-1.5">
                        <span className="truncate font-medium">{a.name}</span>
                        {a.running && <Badge tone="accent">Aberto</Badge>}
                        {a.fromAppStore && <Badge>App Store</Badge>}
                        {a.systemApp && <Badge>macOS</Badge>}
                      </div>
                      <div className="truncate text-[11.5px] text-ink-3">
                        {a.version && `${a.version} · `}
                        {shortPath(a.path, home)}
                      </div>
                    </div>
                    <div className="text-[12px] text-ink-2" title="Último uso">
                      {a.lastUsedAt ? formatAge(a.lastUsedAt) : "sem registro"}
                    </div>
                    <div className="tabular text-right text-[12.5px]">{formatBytes(a.size)}</div>
                    {a.systemApp ? (
                      <span className="w-[86px] text-right text-[11px] text-ink-3" title="Vem com o macOS e é protegido pelo sistema">
                        protegido
                      </span>
                    ) : (
                      <Button size="sm" variant="ghost" onClick={() => setTarget(a)} className="opacity-0 group-hover:opacity-100 focus:opacity-100">
                        Desinstalar…
                      </Button>
                    )}
                  </div>
                ))}
              </Card>
            )}
            <p className="text-[11.5px] text-ink-3">"Último uso" vem do Spotlight e pode faltar para apps que nunca foram abertos pelo Finder ou Dock.</p>
          </>
        )}
      </div>
      {target && (
        <UninstallDialog
          app={target}
          home={home}
          onClose={() => setTarget(null)}
          onRemoved={(path) =>
            update<AppsResult>(MODULE, (res) => {
              const apps = res.apps.filter((a) => a.path !== path);
              return { apps, totalSize: apps.reduce((s, a) => s + a.size, 0) };
            })
          }
        />
      )}
    </div>
  );
}

const JOBS: { module: string; command: string; args: Record<string, unknown> }[] = [
  { module: MODULE, command: "start_app_list", args: {} },
  { module: "leftovers", command: "start_orphan_scan", args: {} },
  // Offline only: the online check asks for consent on the Atualizações page.
  { module: "updates", command: "start_update_check", args: { online: false } },
];

// Intro, scan and results around the module's three analyses. Nothing runs until Analisar.
export function AppsPage() {
  const { start, cancel, reset } = useJobs();
  const apps = useJob<AppsResult>(MODULE);
  const leftovers = useJob<OrphansResult>("leftovers");
  const updates = useJob<UpdatesResult>("updates");
  const go = useNav((s) => s.go);
  const [list, setList] = useState<Filter | null>(null);

  const anyRunning = [apps, leftovers, updates].some((j) => j.status === "running");
  const phase = anyRunning ? "scanning" : apps.status === "done" || apps.status === "failed" ? "results" : "intro";
  const scan = () => JOBS.forEach((j) => start(j.module, j.command, j.args));

  const all = apps.result?.apps ?? [];
  const unused = all.filter(isUnused);
  const unusedSize = unused.reduce((s, a) => s + a.size, 0);
  const withSource = (updates.result?.apps ?? []).filter((a) => a.source !== "none");
  const pending = withSource.filter((a) => a.updateAvailable);
  const checked = !!updates.result?.checkedOnline;
  const icons = (list: { path: string }[], size: number) => (
    <span className="flex items-center gap-3">
      {list.slice(0, 3).map((a) => (
        <AppIcon key={a.path} path={a.path} size={size} />
      ))}
    </span>
  );

  return (
    <ModuleStage
      phase={phase}
      title="Aplicativos"
      hero={appsHero}
      intro="Veja o que está instalado, o que não é usado há meses e o que apps já removidos deixaram para trás. Nada é removido sem a sua confirmação."
      scanning="Procurando aplicativos…"
      detail={apps.status === "running" && apps.stage ? `Medindo os apps (${formatCount(apps.stage.done)} de ${formatCount(apps.stage.total)})` : "Lendo apps, atualizações e restos"}
      onScan={scan}
      onStop={() => JOBS.forEach((j) => cancel(j.module))}
      onBack={() => {
        setList(null);
        JOBS.forEach((j) => reset(j.module));
      }}
    >
      {list ? (
        <div className="h-full overflow-y-auto">
          <AppsList initialFilter={list} onBack={() => setList(null)} />
        </div>
      ) : (
        <>
          {apps.status === "failed" && (
            <div className="px-8">
              <ErrorNote>{apps.error}</ErrorNote>
            </div>
          )}
          <ResultsBento
            headline={`Encontramos ${formatCount(all.length)} apps no seu Mac.`}
            pill={<BentoButton onClick={() => setList("all")}>Gerenciar meus aplicativos</BentoButton>}
            extra={
              <BentoButton onClick={scan}>
                <span className="inline-flex items-center gap-1.5">
                  <RefreshCw className="size-3.5" /> Atualizar
                </span>
              </BentoButton>
            }
            tiles={[
              {
                id: "updates",
                title: checked ? `${formatCount(pending.length)} ${pending.length === 1 ? "atualização disponível" : "atualizações disponíveis"}` : `${formatCount(withSource.length)} apps com fonte de atualização`,
                description: checked ? "Atualize seus apps para ter os recursos e correções mais recentes." : "Verifique online para comparar a versão instalada com a mais recente.",
                visual: icons(checked && pending.length ? pending : withSource, 72),
                actions: (
                  <BentoButton primary onClick={() => go("updates")}>
                    {checked ? "Revisar" : "Verificar"}
                  </BentoButton>
                ),
              },
              {
                id: "unused",
                title: `${formatCount(unused.length)} apps sem uso há 6 meses`,
                description: unused.length ? `Você pode não precisar deles, mas ocupam ${formatBytes(unusedSize)} no total.` : "Todos os seus apps foram abertos nos últimos 6 meses.",
                visual: icons([...unused].sort((a, b) => b.size - a.size), 52),
                actions: (
                  <BentoButton primary disabled={!unused.length} onClick={() => setList("unused")}>
                    Revisar
                  </BentoButton>
                ),
              },
              {
                id: "leftovers",
                title: `${formatBytes(leftovers.result?.totalSize ?? 0)} de restos de apps`,
                description: "Arquivos em ~/Library de apps que não estão mais instalados.",
                actions: (
                  <BentoButton primary onClick={() => go("leftovers")}>
                    Revisar
                  </BentoButton>
                ),
              },
              {
                id: "all",
                title: `${formatBytes(apps.result?.totalSize ?? 0)} em aplicativos`,
                description: "Todos os apps instalados, com tamanho e último uso.",
                actions: <BentoButton onClick={() => setList("all")}>Revisar</BentoButton>,
              },
            ]}
          />
        </>
      )}
    </ModuleStage>
  );
}
