import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { ChevronDown, ChevronRight, Info, Lock, Plus, RefreshCw, X } from "lucide-react";
import { withBackupAck } from "../../stores/backupGate";
import { api, errorMessage } from "../../lib/ipc";
import { Badge, Button, Card, ErrorNote, PageHeader } from "../../components/ui";
import { Modal, ReviewDialog, SelectionBar } from "../../components/ReviewDialog";
import { useHome } from "../scanner/ScannerPage";
import { formatBytes, formatCount, shortPath } from "../../lib/format";
import type { Risk, RuleResult } from "../../types";

const groups: { id: RuleResult["group"]; title: string }[] = [
  { id: "user", title: "Caches e registros" },
  { id: "developer", title: "Ferramentas de desenvolvimento" },
  { id: "browser", title: "Navegadores" },
  { id: "mail", title: "Mail" },
  { id: "custom", title: "Suas regras" },
];

const riskBadge: Record<Risk, React.ReactNode> = {
  low: <Badge tone="safe">Risco baixo</Badge>,
  medium: <Badge tone="review">Risco médio</Badge>,
  high: <Badge tone="danger">Risco alto</Badge>,
};

function RuleCard({ rule, selected, setMany, home, onRemoveCustom }: { rule: RuleResult; selected: Set<string>; setMany: (keys: string[], v: boolean) => void; home?: string; onRemoveCustom?: () => void }) {
  const [open, setOpen] = useState(false);
  const key = (path: string) => `${rule.id}|${path}`;
  const available = rule.items.filter((i) => !i.blocked);
  const chosen = rule.items.filter((i) => selected.has(key(i.path)));
  const all = available.length > 0 && available.every((i) => selected.has(key(i.path)));
  const some = chosen.length > 0;
  return (
    <Card className="overflow-hidden">
      <div className="flex items-start gap-3 px-4 py-3">
        <input
          type="checkbox"
          className="mt-1 size-3.5 accent-[var(--accent)]"
          aria-label={`Selecionar ${rule.title}`}
          disabled={!available.length}
          checked={all}
          ref={(el) => {
            if (el) el.indeterminate = some && !all;
          }}
          onChange={(e) => setMany(available.map((i) => key(i.path)), e.target.checked)}
        />
        <button className="min-w-0 flex-1 text-left" onClick={() => setOpen(!open)} disabled={!rule.items.length}>
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-medium">{rule.title}</span>
            {riskBadge[rule.risk]}
          </div>
          {rule.description && <div className="mt-0.5 text-[12px] text-ink-2">{rule.description}</div>}
          {rule.blockedBy.length > 0 && (
            <div className="mt-1 flex items-center gap-1 text-[12px] text-review">
              <Lock className="size-3" /> Feche {rule.blockedBy.join(", ")} para limpar.
            </div>
          )}
          {rule.unavailable && (
            <div className="mt-1 flex items-center gap-1 text-[12px] text-ink-3">
              <Info className="size-3" /> Precisa de Acesso Total ao Disco.
            </div>
          )}
        </button>
        <div className="text-right">
          <div className="tabular text-[13px] font-semibold">{formatBytes(rule.size)}</div>
          <div className="text-[11px] text-ink-3">{formatCount(rule.items.length)} itens</div>
        </div>
        {onRemoveCustom && (
          <button className="rounded p-1 text-ink-3 hover:bg-surface-2 hover:text-ink" title="Remover regra" onClick={onRemoveCustom}>
            <X className="size-3.5" />
          </button>
        )}
        {rule.items.length > 0 && (
          <button className="pt-0.5 text-ink-3" onClick={() => setOpen(!open)} aria-label="Ver itens">
            {open ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
          </button>
        )}
      </div>
      {open && (
        <div className="max-h-72 overflow-y-auto border-t border-line px-2 py-1">
          {rule.items.map((i) => (
            <label key={i.path} className={`grid grid-cols-[18px_minmax(0,1fr)_auto_80px] items-center gap-3 rounded-md px-2 py-1 ${i.blocked ? "opacity-60" : "cursor-pointer hover:bg-surface-2"}`}>
              <input type="checkbox" className="size-3.5 accent-[var(--accent)]" disabled={!!i.blocked} checked={selected.has(key(i.path))} onChange={(e) => setMany([key(i.path)], e.target.checked)} />
              <span className="truncate text-[12px] selectable" title={i.path}>
                {shortPath(i.path, home)}
              </span>
              <span className="text-[11px]">
                {i.blocked ? <span className="text-review">{i.blocked} aberto</span> : i.system ? <Badge>macOS</Badge> : null}
              </span>
              <span className="tabular text-right text-[12px]">{formatBytes(i.size)}</span>
            </label>
          ))}
        </div>
      )}
    </Card>
  );
}

function AddRuleDialog({ onClose, home }: { onClose: () => void; home?: string }) {
  const qc = useQueryClient();
  const [title, setTitle] = useState("");
  const [folder, setFolder] = useState<string | null>(null);
  const [risk, setRisk] = useState<Risk>("medium");
  const [error, setError] = useState<string | null>(null);
  const save = async () => {
    if (!folder) return;
    try {
      await api.addCustomRule(title || folder.split("/").pop() || "Regra", folder, risk);
      qc.invalidateQueries({ queryKey: ["cleanupRules"] });
      onClose();
    } catch (e) {
      setError(errorMessage(e));
    }
  };
  return (
    <Modal onClose={onClose} width={480}>
      <div className="space-y-4 p-6">
        <div>
          <h2 className="text-[16px] font-semibold">Nova regra de limpeza</h2>
          <p className="mt-1 text-[12.5px] text-ink-2">O conteúdo da pasta escolhida aparece para revisão e vai para a Lixeira quando você limpar. A pasta em si fica.</p>
        </div>
        <label className="block text-[12.5px]">
          <span className="text-ink-2">Nome</span>
          <input value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Builds antigos" className="mt-1 h-8 w-full rounded-lg glass px-3 outline-none focus:border-accent" />
        </label>
        <div className="text-[12.5px]">
          <span className="text-ink-2">Pasta (dentro da pasta pessoal)</span>
          <button
            onClick={async () => {
              const dir = await open({ directory: true, defaultPath: home });
              if (typeof dir === "string") setFolder(dir);
            }}
            className="mt-1 flex h-8 w-full items-center rounded-lg border border-line bg-surface-2 px-3 text-left hover:border-accent/50"
          >
            <span className="truncate">{folder ? shortPath(folder, home) : "Escolher…"}</span>
          </button>
        </div>
        <label className="block text-[12.5px]">
          <span className="text-ink-2">Risco</span>
          <select value={risk} onChange={(e) => setRisk(e.target.value as Risk)} className="mt-1 h-8 w-full rounded-lg glass px-2">
            <option value="low">Baixo: o conteúdo é recriado sozinho</option>
            <option value="medium">Médio: revisar antes</option>
            <option value="high">Alto: dados que não voltam</option>
          </select>
        </label>
        {error && <ErrorNote>{error}</ErrorNote>}
      </div>
      <div className="flex justify-end gap-2 border-t border-line px-6 py-3">
        <Button onClick={onClose}>Cancelar</Button>
        <Button variant="primary" disabled={!folder} onClick={save}>
          Adicionar
        </Button>
      </div>
    </Modal>
  );
}

export function AdvancedCleanupPage() {
  const home = useHome();
  const qc = useQueryClient();
  const { data, isFetching, error, refetch } = useQuery({ queryKey: ["cleanupRules"], queryFn: api.cleanupRules, staleTime: 0 });
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [review, setReview] = useState(false);
  const [adding, setAdding] = useState(false);

  useEffect(() => {
    if (data) setSelected(new Set(data.results.flatMap((r) => r.items.filter((i) => i.selected).map((i) => `${r.id}|${i.path}`))));
  }, [data]);

  const setMany = (keys: string[], v: boolean) =>
    setSelected((prev) => {
      const next = new Set(prev);
      for (const k of keys) {
        if (v) next.add(k);
        else next.delete(k);
      }
      return next;
    });

  // One entry per path (a path can match two rules, e.g. a Chrome cache inside ~/Library/Caches).
  const chosen = useMemo(() => {
    const seen = new Map<string, { ruleId: string; path: string; size: number; title: string }>();
    for (const r of data?.results ?? [])
      for (const i of r.items) if (selected.has(`${r.id}|${i.path}`) && !seen.has(i.path)) seen.set(i.path, { ruleId: r.id, path: i.path, size: i.size, title: r.title });
    // Drop paths inside another chosen path: the parent already covers them.
    const paths = [...seen.keys()];
    return [...seen.values()].filter((x) => !paths.some((p) => p !== x.path && x.path.startsWith(p + "/")));
  }, [data, selected]);
  const bytes = chosen.reduce((s, i) => s + i.size, 0);

  return (
    <div className="h-full overflow-y-auto">
      <PageHeader
        title="Limpeza avançada"
        subtitle="Regras para caches, registros, ferramentas de desenvolvimento, navegadores e Mail. Tudo vai para a Lixeira; apps abertos bloqueiam a limpeza dos próprios dados."
        actions={
          <>
            <Button variant="ghost" icon={<RefreshCw className="size-3.5" />} busy={isFetching} onClick={() => refetch()}>
              Atualizar
            </Button>
            <Button icon={<Plus className="size-3.5" />} onClick={() => setAdding(true)}>
              Nova regra
            </Button>
          </>
        }
      />
      <div className="space-y-6 px-8 pb-4">
        {error && <ErrorNote>{errorMessage(error)}</ErrorNote>}
        {!data && isFetching && <div className="text-ink-3">Calculando o tamanho de cada regra…</div>}
        {data &&
          groups.map((g) => {
            const rules = data.results.filter((r) => r.group === g.id && (r.items.length > 0 || r.blockedBy.length > 0 || r.unavailable || g.id === "custom"));
            if (!rules.length) return null;
            return (
              <section key={g.id} className="space-y-2">
                <h2 className="text-[13px] font-semibold">{g.title}</h2>
                {g.id === "browser" && (
                  <p className="text-[12px] text-ink-3">O navegador precisa estar fechado. O Safari é protegido pelo macOS e não é limpo por aqui; use Safari → Limpar Histórico.</p>
                )}
                {rules.map((r) => (
                  <RuleCard
                    key={r.id}
                    rule={r}
                    selected={selected}
                    setMany={setMany}
                    home={home}
                    onRemoveCustom={
                      r.group === "custom"
                        ? async () => {
                            await api.removeCustomRule(r.id.replace(/^custom:/, ""));
                            qc.invalidateQueries({ queryKey: ["cleanupRules"] });
                          }
                        : undefined
                    }
                  />
                ))}
              </section>
            );
          })}
        {data && !data.results.some((r) => r.group === "custom") && (
          <p className="text-[12px] text-ink-3">Crie regras para pastas que você limpa sempre, como builds antigos ou exportações temporárias.</p>
        )}
        {data && <SelectionBar count={chosen.length} bytes={bytes} onReview={() => setReview(true)} />}
      </div>
      {review && (
        <ReviewDialog
          items={chosen.map((c) => ({ path: c.path, size: c.size, label: c.title }))}
          home={home}
          execute={() => withBackupAck((ack) => api.runCleanupRules(chosen.map(({ ruleId, path, size }) => ({ ruleId, path, size })), ack))}
          onClose={() => setReview(false)}
          onDone={() => qc.invalidateQueries({ queryKey: ["cleanupRules"] })}
        />
      )}
      {adding && <AddRuleDialog home={home} onClose={() => setAdding(false)} />}
    </div>
  );
}
