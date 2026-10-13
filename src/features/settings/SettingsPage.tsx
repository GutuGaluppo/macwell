import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { Plus, X } from "lucide-react";
import { api } from "../../lib/ipc";
import { Button, Card, PageHeader } from "../../components/ui";
import { FullDiskAccessRow } from "../onboarding/Onboarding";
import { formatBytes, formatCount, formatDateTime, shortPath } from "../../lib/format";
import { useHome } from "../scanner/ScannerPage";
import { useRemovalMode } from "../../lib/removalMode";
import type { MenuBarSettings, RemovalMode } from "../../types";

function Section({ title, description, children }: { title: string; description?: string; children: React.ReactNode }) {
  return (
    <div>
      <h2 className="text-[13px] font-semibold">{title}</h2>
      {description && <p className="mt-0.5 mb-2.5 text-[12px] text-ink-3">{description}</p>}
      <Card className="p-4">{children}</Card>
    </div>
  );
}

function RemovalModeSection() {
  const qc = useQueryClient();
  const { mode } = useRemovalMode();
  const choose = async (next: RemovalMode) => {
    await api.setRemovalMode(next);
    qc.invalidateQueries({ queryKey: ["removalMode"] });
  };
  const options: { id: RemovalMode; title: string; text: string }[] = [
    { id: "trash", title: "Lixeira (padrão)", text: "Os itens vão para a Lixeira do macOS e saem de vez só quando você a esvazia." },
    {
      id: "quarantine",
      title: "Quarentena",
      text: "Os itens são movidos (sem cópia) para uma pasta com a data, no mesmo disco, com um registro em CSV. Continuam ocupando espaço até você enviá-los à Lixeira. Itens em outro disco ficam de fora e pedem a Lixeira.",
    },
  ];
  return (
    <div className="space-y-2" role="radiogroup" aria-label="Destino dos itens removidos">
      {options.map((o) => (
        <label key={o.id} className="flex cursor-pointer items-start gap-3 rounded-lg px-1 py-1.5 hover:bg-surface-2">
          <input type="radio" name="removal-mode" className="mt-1 accent-[var(--accent)]" checked={mode === o.id} onChange={() => choose(o.id)} />
          <span>
            <span className="block text-[13px] font-medium">{o.title}</span>
            <span className="block text-[12px] text-ink-3">{o.text}</span>
          </span>
        </label>
      ))}
    </div>
  );
}

function MenuBarSection() {
  const qc = useQueryClient();
  const { data } = useQuery({ queryKey: ["menuBar"], queryFn: api.menuBarSettings });
  const save = async (next: MenuBarSettings) => {
    await api.setMenuBarSettings(next);
    qc.setQueryData(["menuBar"], next);
  };
  if (!data) return null;
  return (
    <div className="space-y-3">
      <label className="flex items-center gap-2 text-[12.5px]">
        <input type="checkbox" className="accent-[var(--accent)]" checked={data.enabled} onChange={(e) => save({ ...data, enabled: e.target.checked })} />
        Mostrar o OrganizeMyMac na barra de menus
      </label>
      <div className="flex items-center gap-2 text-[12.5px]">
        <span className="text-ink-2">Ao lado do ícone:</span>
        <select
          disabled={!data.enabled}
          value={data.title}
          onChange={(e) => save({ ...data, title: e.target.value as MenuBarSettings["title"] })}
          className="h-7 rounded-md glass px-2 disabled:opacity-50"
        >
          <option value="icon">Nada (só o ícone)</option>
          <option value="cpu">Uso de CPU</option>
          <option value="memory">Uso de memória</option>
          <option value="both">CPU e memória</option>
        </select>
      </div>
      <p className="text-[11.5px] text-ink-3">
        O menu mostra CPU, memória, disco, bateria e os apps que mais usam memória. Atualiza a cada 5 segundos com prioridade baixa. Com ele
        ligado, fechar a janela mantém o app na barra de menus.
      </p>
    </div>
  );
}

function IgnoreList() {
  const qc = useQueryClient();
  const home = useHome();
  const { data } = useQuery({ queryKey: ["ignore"], queryFn: api.ignoreList });
  const add = async () => {
    const dir = await open({ directory: true, title: "Pasta que o OrganizeMyMac deve ignorar" });
    if (typeof dir === "string") {
      await api.addToIgnoreList(dir, "adicionada em Ajustes");
      qc.invalidateQueries({ queryKey: ["ignore"] });
    }
  };
  const remove = async (path: string) => {
    await api.removeFromIgnoreList(path);
    qc.invalidateQueries({ queryKey: ["ignore"] });
  };
  return (
    <div className="space-y-2">
      {data?.length ? (
        data.map((e) => (
          <div key={e.path} className="flex items-center gap-2 text-[12.5px]">
            <span className="flex-1 truncate selectable" title={e.path}>
              {shortPath(e.path, home)}
            </span>
            <button className="rounded p-1 text-ink-3 hover:bg-surface-2 hover:text-ink" onClick={() => remove(e.path)} aria-label="Remover">
              <X className="size-3.5" />
            </button>
          </div>
        ))
      ) : (
        <div className="text-[12.5px] text-ink-3">Nenhum item ignorado.</div>
      )}
      <Button size="sm" icon={<Plus className="size-3.5" />} onClick={add}>
        Adicionar pasta…
      </Button>
    </div>
  );
}

function OperationLog() {
  const home = useHome();
  const { data } = useQuery({ queryKey: ["operations"], queryFn: () => api.operationLog(100) });
  if (!data?.length) return <div className="text-[12.5px] text-ink-3">Nenhuma remoção ainda.</div>;
  const actions: Record<string, string> = { trash: "Lixeira", emptyTrash: "Apagado da Lixeira", uninstall: "Desinstalação" };
  return (
    <div className="max-h-72 space-y-1 overflow-y-auto">
      {data.map((op) => (
        <div key={op.id} className="grid grid-cols-[110px_minmax(0,1fr)_70px_80px] gap-3 text-[12px]">
          <span className="text-ink-3">{formatDateTime(op.ts)}</span>
          <span className="truncate selectable" title={op.detail ?? op.path}>
            {shortPath(op.path, home)}
          </span>
          <span className="tabular text-right text-ink-2">{formatBytes(op.size)}</span>
          <span className={op.ok ? "text-ink-3" : "text-danger"}>{op.ok ? actions[op.action] ?? op.action : "Falhou"}</span>
        </div>
      ))}
    </div>
  );
}

function Metrics() {
  const { data } = useQuery({ queryKey: ["metrics"], queryFn: api.localMetrics });
  const m = data ?? {};
  const rows: [string, string][] = [
    ["Análises feitas", formatCount(m.scans_total ?? 0)],
    ["Arquivos analisados", formatCount(m.files_scanned_total ?? 0)],
    ["Espaço liberado", formatBytes(m.bytes_reclaimed_total ?? 0)],
    ["Itens removidos", formatCount(m.items_removed_total ?? 0)],
    ["Erros ao remover", formatCount(m.operation_errors_total ?? 0)],
  ];
  return (
    <div className="grid grid-cols-5 gap-3">
      {rows.map(([k, v]) => (
        <div key={k}>
          <div className="text-[11px] text-ink-3">{k}</div>
          <div className="tabular text-[15px] font-semibold">{v}</div>
        </div>
      ))}
    </div>
  );
}

export function SettingsPage() {
  const { data: info } = useQuery({ queryKey: ["appInfo"], queryFn: api.appInfo });
  const home = useHome();
  return (
    <div className="h-full overflow-y-auto">
      <PageHeader title="Ajustes" />
      <div className="max-w-3xl space-y-6 px-8 pb-10">
        <Section title="Permissões" description="O app funciona sem Acesso Total ao Disco, mas não enxerga pastas protegidas pelo macOS.">
          <FullDiskAccessRow compact />
        </Section>
        <Section title="Ao remover itens" description="Vale para a limpeza em lote e para as revisões. Desinstalar um app continua indo para a Lixeira.">
          <RemovalModeSection />
        </Section>
        <Section title="Barra de menus">
          <MenuBarSection />
        </Section>
        <Section title="Itens ignorados" description="Pastas que nenhuma análise lê e que nunca são sugeridas para remoção.">
          <IgnoreList />
        </Section>
        <Section title="Registro de operações" description="Tudo o que o app moveu para a Lixeira ou apagou, guardado só neste Mac.">
          <OperationLog />
        </Section>
        <Section title="Estatísticas locais" description="Calculadas e guardadas só neste Mac. Nada é enviado.">
          <Metrics />
        </Section>
        {info && (
          <div className="space-y-0.5 text-[11.5px] text-ink-3 selectable">
            <div>OrganizeMyMac {info.version}</div>
            <div>Dados: {shortPath(info.dataDir, home)}</div>
            <div>Registros: {shortPath(info.logDir, home)}</div>
          </div>
        )}
      </div>
    </div>
  );
}
