import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { Boxes, Camera, ChevronDown, ChevronLeft, ChevronRight, FolderOpen, FolderPlus, GitBranch, Send, X } from "lucide-react";
import { api, errorMessage } from "../../lib/ipc";
import { withBackupAck } from "../../stores/backupGate";
import { useJob, useJobs } from "../../stores/jobs";
import { Badge, Button, Card, ErrorNote, PageHeader, Stat } from "../../components/ui";
import { ModuleStage } from "../../components/ModuleStage";
import { BentoButton, BentoIcon, ResultsBento } from "../../components/ResultsBento";
import projectsHero from "../../assets/icons/projetos-hd.webp";
import { ReviewDialog, SelectionBar } from "../../components/ReviewDialog";
import { useHome } from "../scanner/ScannerPage";
import { formatAge, formatBytes, formatCount, formatDateTime, shortPath } from "../../lib/format";
import type { DevArtifact, DevReport, OperationOutcome, RepoReport } from "../../types";
import { artifactKindLabel, noteLabel, remoteLabel, unsyncedReasons } from "./labels";
import { SnapshotDialog } from "./SnapshotDialog";
import { PushDialog } from "./PushDialog";

const MODULE = "dev";

const baseName = (p: string) => p.split("/").filter(Boolean).pop() ?? p;

function ArtifactRows({
  artifacts,
  home,
  picked,
  onToggle,
  onPickSafe,
}: {
  artifacts: DevArtifact[];
  home?: string;
  picked: Set<string>;
  onToggle: (path: string) => void;
  onPickSafe: () => void;
}) {
  if (!artifacts.length) return <div className="px-4 py-3 text-[12px] text-ink-3">Nenhuma pasta regenerável encontrada neste repositório.</div>;
  return (
    <div className="divide-y divide-line bg-black/5">
      <div className="flex items-center gap-2 px-4 py-1.5 pl-12 text-[11.5px] text-ink-3">
        Nada vem marcado: você escolhe o que mover para a Lixeira.
        <Button size="sm" variant="ghost" onClick={onPickSafe}>
          Marcar os seguros
        </Button>
      </div>
      {artifacts.map((a) => (
        <div key={a.path} className="grid grid-cols-[18px_minmax(0,1.3fr)_90px_110px_minmax(0,1.6fr)_28px] items-start gap-3 px-4 py-2 pl-5">
          <input
            type="checkbox"
            className="mt-1 size-3.5 accent-[var(--accent)] disabled:opacity-40"
            checked={picked.has(a.path)}
            disabled={a.notes.includes("tracked")}
            title={a.notes.includes("tracked") ? "Tem arquivos versionados: não pode ser removida por aqui" : undefined}
            onChange={() => onToggle(a.path)}
          />
          <div className="min-w-0">
            <div className="flex items-center gap-1.5">
              <span className="font-medium">{artifactKindLabel[a.kind] ?? a.kind}</span>
              <Badge tone={a.confidence === "safe" ? "safe" : "review"}>{a.confidence === "safe" ? "Seguro" : "Revisar"}</Badge>
            </div>
            <div className="truncate font-mono text-[11px] text-ink-3 selectable" title={shortPath(a.path, home)}>
              {a.path.startsWith(`${a.repo}/`) ? a.path.slice(a.repo.length + 1) : shortPath(a.path, home)}
            </div>
          </div>
          <div className="tabular text-right font-medium">{formatBytes(a.size)}</div>
          <div className="text-[12px] text-ink-2" title="O mais recente entre o manifest, o lockfile e o último commit">
            {formatAge(a.lastUsedMs)}
          </div>
          <div className="text-[11.5px] text-ink-2">
            <div>{a.reason}</div>
            {a.notes.map((n) => (
              <div key={n} className="text-review">
                {noteLabel[n]}
              </div>
            ))}
          </div>
          <button className="text-ink-3 hover:text-ink" title="Mostrar no Finder" onClick={() => api.revealInFinder(a.path)}>
            <FolderOpen className="size-4" />
          </button>
        </div>
      ))}
    </div>
  );
}

function RepoRow({
  repo,
  artifacts,
  home,
  expanded,
  picked,
  onToggle,
  onPick,
  onPickSafe,
  onSnapshot,
  onPush,
}: {
  repo: RepoReport;
  artifacts: DevArtifact[];
  home?: string;
  expanded: boolean;
  picked: Set<string>;
  onToggle: () => void;
  onPick: (path: string) => void;
  onPickSafe: () => void;
  onSnapshot: () => void;
  onPush: () => void;
}) {
  const s = repo.status;
  const reasons = unsyncedReasons(repo);
  const remote = repo.remotes[0];
  return (
    <div>
      <div className="grid grid-cols-[minmax(0,1.5fr)_minmax(0,1.3fr)_minmax(0,1fr)_auto] items-center gap-4 px-4 py-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-1.5">
            <span className="truncate font-medium">{baseName(repo.path)}</span>
            {repo.gitFile && <Badge>worktree ou submódulo</Badge>}
            {repo.parent && <Badge>dentro de {baseName(repo.parent)}</Badge>}
            {repo.submodules.length > 0 && <Badge>{repo.submodules.length} submódulo{repo.submodules.length > 1 ? "s" : ""}</Badge>}
            {repo.embedded.length > 0 && (
              <span title={`Repositório adicionado sem registro em .gitmodules: ${repo.embedded.join(", ")}`}>
                <Badge tone="review">{repo.embedded.length} embutido{repo.embedded.length > 1 ? "s" : ""}</Badge>
              </span>
            )}
          </div>
          <div className="truncate font-mono text-[11px] text-ink-3 selectable" title={repo.path}>
            {shortPath(repo.path, home)}
          </div>
          {repo.lastSnapshot && (
            <div className="mt-0.5 text-[11px] text-ink-3">
              Último snapshot: <span className="font-mono">{repo.lastSnapshot.refName}</span> · {repo.lastSnapshot.pushedAt ? "enviado" : "só local"}
            </div>
          )}
        </div>
        <div className="min-w-0 text-[12.5px]">
          {repo.error && !s ? (
            <span className="text-danger" title={repo.error}>
              Não foi possível ler
            </span>
          ) : s ? (
            <>
              <div className="flex items-center gap-1.5">
                <GitBranch className="size-3.5 shrink-0 text-ink-3" />
                <span className="truncate font-mono">{s.detached ? "(HEAD solto)" : (s.branch ?? "—")}</span>
                {s.ahead !== null && s.behind !== null && (s.ahead > 0 || s.behind > 0) && (
                  <span className="tabular text-ink-2">
                    ↑{s.ahead} ↓{s.behind}
                  </span>
                )}
              </div>
              <div className="text-[11.5px] text-ink-3">
                {remote ? `${remoteLabel(remote.kind)} · ${remote.name}` : "sem remote"}
                {repo.remotes.length > 1 ? ` (+${repo.remotes.length - 1})` : ""}
              </div>
            </>
          ) : null}
        </div>
        <div className="flex flex-wrap gap-1">
          {s && reasons.length === 0 && <Badge tone="safe">Em dia</Badge>}
          {reasons.map((r) => (
            <Badge key={r} tone="review">
              {r}
            </Badge>
          ))}
        </div>
        <div className="flex items-center gap-1.5">
          {artifacts.length > 0 && (
            <button onClick={onToggle} className="flex items-center gap-1 rounded-md px-1.5 py-1 text-[12px] text-ink-2 hover:bg-white/10" aria-expanded={expanded}>
              {expanded ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
              {formatBytes(repo.artifactBytes)} regenerável
            </button>
          )}
          {repo.lastSnapshot && !repo.lastSnapshot.pushedAt && repo.remotes.length > 0 && (
            <Button size="sm" icon={<Send className="size-3.5" />} onClick={onPush}>
              Enviar
            </Button>
          )}
          <Button size="sm" icon={<Camera className="size-3.5" />} onClick={onSnapshot} disabled={!s}>
            Snapshot
          </Button>
          <button className="text-ink-3 hover:text-ink" title="Mostrar no Finder" onClick={() => api.revealInFinder(repo.path)}>
            <FolderOpen className="size-4" />
          </button>
        </div>
      </div>
      {expanded && <ArtifactRows artifacts={artifacts} home={home} picked={picked} onToggle={onPick} onPickSafe={onPickSafe} />}
    </div>
  );
}

function ProjectsDetails({ onBack, initialOnlyPending }: { onBack: () => void; initialOnlyPending: boolean }) {
  const home = useHome();
  const job = useJob<DevReport>(MODULE);
  const patchResult = useJobs((s) => s.update);
  const [onlyPending, setOnlyPending] = useState(initialOnlyPending);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [snapshotOf, setSnapshotOf] = useState<RepoReport | null>(null);
  const [pushOf, setPushOf] = useState<{ repo: RepoReport; refName: string } | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [reviewing, setReviewing] = useState(false);
  const r = job.result;

  const repos = (r?.repos ?? []).filter((x) => !onlyPending || unsyncedReasons(x).length > 0);
  const pendingCount = (r?.repos ?? []).filter((x) => unsyncedReasons(x).length > 0).length;
  const regenerable = (r?.artifacts ?? []).reduce((s, a) => s + a.size, 0);
  const safeBytes = (r?.artifacts ?? []).filter((a) => a.confidence === "safe").reduce((s, a) => s + a.size, 0);
  const toggle = (path: string) =>
    setExpanded((cur) => {
      const next = new Set(cur);
      if (!next.delete(path)) next.add(path);
      return next;
    });
  const pick = (path: string) =>
    setPicked((cur) => {
      const next = new Set(cur);
      if (!next.delete(path)) next.add(path);
      return next;
    });
  const pickSafe = (repoPath: string) =>
    setPicked((cur) => {
      const next = new Set(cur);
      (r?.artifacts ?? []).filter((a) => a.repo === repoPath && a.confidence === "safe").forEach((a) => next.add(a.path));
      return next;
    });
  const chosen = (r?.artifacts ?? []).filter((a) => picked.has(a.path));
  const removed = (outcomes: OperationOutcome[]) => {
    const gone = new Set(outcomes.filter((o) => o.ok).map((o) => o.path));
    if (!gone.size) return;
    setPicked((cur) => new Set([...cur].filter((p) => !gone.has(p))));
    patchResult<DevReport>(MODULE, (res) => {
      const artifacts = res.artifacts.filter((a) => !gone.has(a.path));
      return {
        ...res,
        artifacts,
        repos: res.repos.map((x) => ({ ...x, artifactBytes: artifacts.filter((a) => a.repo === x.path).reduce((n, a) => n + a.size, 0) })),
      };
    });
  };
  const markSnapshot = (repo: string, refName: string, commitId: string) =>
    patchResult<DevReport>(MODULE, (res) => ({
      ...res,
      repos: res.repos.map((x) =>
        x.path === repo ? { ...x, lastSnapshot: { id: "", repo, refName, commitId, createdAt: Date.now(), pushedAt: null } } : x,
      ),
    }));
  const markPushed = (repo: string) =>
    patchResult<DevReport>(MODULE, (res) => ({
      ...res,
      repos: res.repos.map((x) => (x.path === repo && x.lastSnapshot ? { ...x, lastSnapshot: { ...x.lastSnapshot, pushedAt: Date.now() } } : x)),
    }));

  return (
    <div className="h-full overflow-y-auto">
      <PageHeader
        title="Projetos"
        subtitle="Repositórios git nas pastas que você escolher: o que ainda não foi enviado, o que dá para regenerar e um snapshot seguro do trabalho em andamento. A análise só lê; nada muda sem a sua confirmação."
        actions={
          <Button variant="ghost" icon={<ChevronLeft className="size-3.5" />} onClick={onBack}>
            Voltar aos resultados
          </Button>
        }
      />
      <div className="space-y-4 px-8 pb-10">
        {job.status === "failed" && <ErrorNote>{job.error}</ErrorNote>}

        {r && (
          <>
            {r.gitError && <ErrorNote>{r.gitError}</ErrorNote>}
            <Card className="grid grid-cols-3 gap-6 p-5">
              <Stat label="Repositórios" value={formatCount(r.repos.length)} hint={`em ${r.roots.length} ${r.roots.length === 1 ? "pasta" : "pastas"}`} />
              <Stat label="Com trabalho não enviado" value={formatCount(pendingCount)} hint="alterações, commits ou branch sem destino" />
              <Stat label="Regenerável" value={formatBytes(regenerable)} hint={`${formatBytes(safeBytes)} marcados como seguros`} />
            </Card>
            <div className="flex items-center gap-3">
              <label className="flex items-center gap-2 text-[12.5px] text-ink-2">
                <input type="checkbox" className="accent-[var(--accent)]" checked={onlyPending} onChange={(e) => setOnlyPending(e.target.checked)} />
                Só os com trabalho não enviado
              </label>
              <div className="flex-1" />
              <span className="text-[11.5px] text-ink-3">Analisado em {formatDateTime(r.scannedAt)}</span>
            </div>
            <Card className="divide-y divide-line">
              {repos.length === 0 && <div className="px-4 py-8 text-center text-ink-3">Tudo enviado. Nenhum repositório com trabalho pendente.</div>}
              {repos.map((repo) => (
                <RepoRow
                  key={repo.path}
                  repo={repo}
                  home={home}
                  artifacts={r.artifacts.filter((a) => a.repo === repo.path)}
                  expanded={expanded.has(repo.path)}
                  picked={picked}
                  onToggle={() => toggle(repo.path)}
                  onPick={pick}
                  onPickSafe={() => pickSafe(repo.path)}
                  onSnapshot={() => setSnapshotOf(repo)}
                  onPush={() => repo.lastSnapshot && setPushOf({ repo, refName: repo.lastSnapshot.refName })}
                />
              ))}
            </Card>
            <SelectionBar count={chosen.length} bytes={chosen.reduce((n, a) => n + a.size, 0)} onReview={() => setReviewing(true)} />
          </>
        )}
      </div>

      {reviewing && (
        <ReviewDialog
          home={home}
          items={chosen.map((a) => ({
            path: a.path,
            size: a.size,
            label: `${artifactKindLabel[a.kind] ?? a.kind} · ${baseName(a.repo)}`,
            confidence: a.confidence,
          }))}
          execute={(plain) => {
            const byPath = new Map(chosen.map((a) => [a.path, a]));
            const items = plain.map((i) => ({ path: i.path, size: i.size, repo: byPath.get(i.path)?.repo ?? "", kind: byPath.get(i.path)?.kind ?? "" }));
            return withBackupAck((ack) => api.removeDevArtifacts(items, ack));
          }}
          onClose={() => setReviewing(false)}
          onDone={removed}
        />
      )}
      {snapshotOf && (
        <SnapshotDialog
          repo={snapshotOf}
          home={home}
          onClose={() => setSnapshotOf(null)}
          onCreated={(res) => markSnapshot(snapshotOf.path, res.refName, res.commit)}
          onPush={(refName) => {
            setPushOf({ repo: snapshotOf, refName });
            setSnapshotOf(null);
          }}
        />
      )}
      {pushOf && <PushDialog repo={pushOf.repo} refName={pushOf.refName} onClose={() => setPushOf(null)} onPushed={() => markPushed(pushOf.repo.path)} />}
    </div>
  );
}

// The folders the analysis reads, as removable chips plus an add button.
function RootChips() {
  const home = useHome();
  const qc = useQueryClient();
  const { data: roots = [] } = useQuery({ queryKey: ["devRoots"], queryFn: api.devRoots });
  const [error, setError] = useState<string | null>(null);
  const refresh = () => qc.invalidateQueries({ queryKey: ["devRoots"] });
  const add = async () => {
    setError(null);
    try {
      const dir = await open({ directory: true, multiple: false, title: "Escolha uma pasta com seus projetos" });
      if (typeof dir !== "string") return;
      await api.addDevRoot(dir);
      refresh();
    } catch (e) {
      setError(errorMessage(e));
    }
  };
  return (
    <div className="flex max-w-[640px] flex-wrap items-center justify-center gap-2">
      {roots.map((root) => (
        <span key={root.path} className="glass flex h-8 max-w-[260px] items-center gap-1.5 rounded-full pr-1.5 pl-3 text-[12.5px]" title={root.path}>
          <span className="truncate">{shortPath(root.path, home)}</span>
          <button
            className="grid size-5 shrink-0 place-items-center rounded-full text-ink-3 hover:bg-white/15 hover:text-ink"
            title="Remover da lista (não apaga nada)"
            aria-label={`Remover ${shortPath(root.path, home)}`}
            onClick={async () => {
              await api.removeDevRoot(root.path);
              refresh();
            }}
          >
            <X className="size-3.5" />
          </button>
        </span>
      ))}
      <button onClick={add} className="glass flex h-8 items-center gap-1.5 rounded-full px-3.5 text-[12.5px] font-medium hover:bg-white/20">
        <FolderPlus className="size-3.5" /> {roots.length ? "Adicionar pasta" : "Adicionar pasta de projetos"}
      </button>
      {error && <div className="w-full text-[12px] text-danger">{error}</div>}
    </div>
  );
}

// Intro, scan and results around the git analysis. It needs at least one folder to read.
export function ProjectsPage() {
  const job = useJob<DevReport>(MODULE);
  const { start, cancel, reset } = useJobs();
  const { data: roots = [] } = useQuery({ queryKey: ["devRoots"], queryFn: api.devRoots });
  const [details, setDetails] = useState<{ onlyPending: boolean } | null>(null);
  const r = job.result;
  const phase = job.status === "running" ? "scanning" : job.status === "done" || job.status === "failed" ? "results" : "intro";

  const repos = r?.repos ?? [];
  const pending = repos.filter((x) => unsyncedReasons(x).length > 0);
  const artifacts = r?.artifacts ?? [];
  const regenerable = artifacts.reduce((n, a) => n + a.size, 0);
  const safeBytes = artifacts.filter((a) => a.confidence === "safe").reduce((n, a) => n + a.size, 0);
  const localSnapshots = repos.filter((x) => x.lastSnapshot && !x.lastSnapshot.pushedAt).length;
  const byKind = Object.entries(
    artifacts.reduce<Record<string, number>>((acc, a) => {
      acc[a.kind] = (acc[a.kind] ?? 0) + a.size;
      return acc;
    }, {}),
  )
    .sort((x, y) => y[1] - x[1])
    .slice(0, 4);
  const scan = () => start(MODULE, "start_dev_scan", {});
  const stage = job.stage?.stage === "repos" ? job.stage : undefined;

  return (
    <ModuleStage
      phase={phase}
      title="Projetos"
      hero={projectsHero}
      intro="Encontre repositórios git com trabalho não enviado e pastas que dá para regenerar. A análise só lê; nada muda sem a sua confirmação."
      introExtra={
        <div className="flex flex-col items-center gap-2">
          {roots.length === 0 && <span className="text-[12.5px] text-ink-3">Escolha a pasta onde ficam seus repositórios para começar.</span>}
          <RootChips />
        </div>
      }
      scanDisabled={roots.length === 0}
      scanning="Procurando repositórios…"
      detail={stage ? `Lendo repositórios (${formatCount(stage.done)} de ${formatCount(stage.total)})` : "Procurando repositórios"}
      progress={stage ? stage.done / Math.max(stage.total, 1) : undefined}
      onScan={scan}
      onStop={() => {
        cancel(MODULE);
        reset(MODULE);
      }}
      onBack={() => {
        setDetails(null);
        reset(MODULE);
      }}
    >
      {details ? (
        <ProjectsDetails initialOnlyPending={details.onlyPending} onBack={() => setDetails(null)} />
      ) : (
        <>
          {job.status === "failed" && (
            <div className="px-8">
              <ErrorNote>{job.error}</ErrorNote>
            </div>
          )}
          {r?.gitError && (
            <div className="px-8">
              <ErrorNote>{r.gitError}</ErrorNote>
            </div>
          )}
          {r && (
            <ResultsBento
              headline={pending.length ? `${formatCount(pending.length)} ${pending.length === 1 ? "projeto tem" : "projetos têm"} trabalho que só existe no seu Mac.` : "Tudo enviado. Nenhum trabalho só no seu Mac."}
              pill={<BentoButton onClick={() => setDetails({ onlyPending: false })}>Ver todos os repositórios</BentoButton>}
              extra={<BentoButton onClick={scan}>Analisar de novo</BentoButton>}
              tiles={[
                {
                  id: "pending",
                  title: `${formatCount(pending.length)} de ${formatCount(repos.length)} repositórios com trabalho não enviado`,
                  description: "Alterações sem commit, commits sem push ou um branch sem destino.",
                  visual: <BentoIcon size={112}><GitBranch /></BentoIcon>,
                  body: (
                    <ul className="space-y-1 text-[12.5px]">
                      {pending.slice(0, 3).map((x) => (
                        <li key={x.path} className="flex justify-between gap-3">
                          <span className="truncate text-ink-2">{baseName(x.path)}</span>
                          <span className="shrink-0 text-ink-3">{unsyncedReasons(x)[0]}</span>
                        </li>
                      ))}
                    </ul>
                  ),
                  actions: (
                    <BentoButton primary disabled={!pending.length} onClick={() => setDetails({ onlyPending: true })}>
                      Revisar
                    </BentoButton>
                  ),
                },
                {
                  id: "regenerable",
                  title: `${formatBytes(regenerable)} em pastas que dá para regenerar`,
                  description: `${formatBytes(safeBytes)} marcados como seguros. Nada vem marcado: você escolhe o que mover para a Lixeira.`,
                  visual: <BentoIcon size={56}><Boxes /></BentoIcon>,
                  body: (
                    <div className="flex flex-wrap gap-1.5">
                      {byKind.map(([kind, size]) => (
                        <Badge key={kind}>
                          {artifactKindLabel[kind] ?? kind} · {formatBytes(size)}
                        </Badge>
                      ))}
                    </div>
                  ),
                  actions: (
                    <BentoButton primary disabled={!artifacts.length} onClick={() => setDetails({ onlyPending: false })}>
                      Revisar
                    </BentoButton>
                  ),
                },
                {
                  id: "repos",
                  title: `${formatCount(repos.length)} ${repos.length === 1 ? "repositório" : "repositórios"}`,
                  description: `em ${formatCount(r?.roots.length ?? 0)} ${(r?.roots.length ?? 0) === 1 ? "pasta" : "pastas"} de projetos.`,
                  visual: <BentoIcon size={48}><FolderOpen /></BentoIcon>,
                  actions: <BentoButton onClick={() => setDetails({ onlyPending: false })}>Revisar</BentoButton>,
                },
                {
                  id: "snapshots",
                  title: `${formatCount(localSnapshots)} ${localSnapshots === 1 ? "snapshot só local" : "snapshots só locais"}`,
                  description: "Um snapshot guarda o trabalho em andamento. Envie para ter uma cópia fora do Mac.",
                  visual: <BentoIcon size={48}><Camera /></BentoIcon>,
                  actions: (
                    <BentoButton primary disabled={!pending.length} onClick={() => setDetails({ onlyPending: true })}>
                      Revisar
                    </BentoButton>
                  ),
                },
              ]}
            />
          )}
        </>
      )}
    </ModuleStage>
  );
}
