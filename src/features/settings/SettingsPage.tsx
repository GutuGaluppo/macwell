import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { EyeOff, MenuSquare, Plus, ShieldCheck, Trash2, X } from "lucide-react";
import { api } from "../../lib/ipc";
import { Button } from "../../components/ui";
import { Modal } from "../../components/ReviewDialog";
import { BentoButton, BentoIcon, ResultsBento } from "../../components/ResultsBento";
import { FullDiskAccessRow } from "../onboarding/Onboarding";
import { formatBytes, formatCount, formatDateTime, shortPath } from "../../lib/format";
import { useHome } from "../scanner/ScannerPage";
import { useRemovalMode } from "../../lib/removalMode";
import type { MenuBarSettings, RemovalMode } from "../../types";

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

type Panel = "removal" | "permissions" | "menuBar" | "ignored" | "log" | "metrics";

const panels: Record<Panel, { title: string; description?: string }> = {
  removal: { title: "Ao remover itens", description: "Vale para a limpeza em lote e para as revisões. Desinstalar um app continua indo para a Lixeira." },
  permissions: { title: "Permissões", description: "O app funciona sem Acesso Total ao Disco, mas não enxerga pastas protegidas pelo macOS." },
  menuBar: { title: "Barra de menus" },
  ignored: { title: "Itens ignorados", description: "Pastas que nenhuma análise lê e que nunca são sugeridas para remoção." },
  log: { title: "Registro de operações", description: "Tudo o que o app moveu para a Lixeira ou apagou, guardado só neste Mac." },
  metrics: { title: "Estatísticas locais", description: "Calculadas e guardadas só neste Mac. Nada é enviado." },
};

function PanelModal({ panel, onClose }: { panel: Panel; onClose: () => void }) {
  const { title, description } = panels[panel];
  return (
    <Modal onClose={onClose} width={panel === "log" ? 760 : 580}>
      <div className="flex items-start justify-between gap-4 px-6 pt-5 pb-3">
        <div>
          <h2 className="text-[16px] font-semibold">{title}</h2>
          {description && <p className="mt-1 text-[12.5px] text-ink-2">{description}</p>}
        </div>
        <button className="rounded p-1 text-ink-3 hover:bg-surface-2 hover:text-ink" onClick={onClose} aria-label="Fechar">
          <X className="size-4" />
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-6">
        {panel === "removal" && <RemovalModeSection />}
        {panel === "permissions" && <FullDiskAccessRow compact />}
        {panel === "menuBar" && <MenuBarSection />}
        {panel === "ignored" && <IgnoreList />}
        {panel === "log" && <OperationLog />}
        {panel === "metrics" && <Metrics />}
      </div>
    </Modal>
  );
}

const menuBarTitle = { icon: "só o ícone", cpu: "uso de CPU", memory: "uso de memória", both: "CPU e memória" };

// Each setting is a tile with its current value; the controls open in a dialog.
export function SettingsPage() {
  const home = useHome();
  const [panel, setPanel] = useState<Panel | null>(null);
  const { data: info } = useQuery({ queryKey: ["appInfo"], queryFn: api.appInfo });
  const { data: perm, refetch: recheck, isFetching: checking } = useQuery({ queryKey: ["permissions"], queryFn: api.permissionStatus, refetchOnWindowFocus: true });
  const { data: bar } = useQuery({ queryKey: ["menuBar"], queryFn: api.menuBarSettings });
  const { data: ignored } = useQuery({ queryKey: ["ignore"], queryFn: api.ignoreList });
  const { mode } = useRemovalMode();

  const access = perm?.fullDiskAccess ?? "unknown";
  const accessLabel = { granted: "concedido", denied: "não concedido", unknown: "não foi possível verificar" }[access];
  const open = (p: Panel) => () => setPanel(p);

  return (
    <div className="flex h-full flex-col">
      <header className="drag flex h-[52px] shrink-0 items-center justify-center text-[13px] font-medium text-ink-2">Ajustes</header>
      <div className="min-h-0 flex-1">
        <ResultsBento
          headline="Deixe o OrganizeMyMac do seu jeito."
          pill={<BentoButton onClick={open("log")}>Registro de operações</BentoButton>}
          extra={<BentoButton onClick={open("metrics")}>Estatísticas locais</BentoButton>}
          footer={
            info && (
              <div className="space-y-0.5 text-center text-[11.5px] text-ink-3 selectable">
                <div>OrganizeMyMac {info.version}</div>
                <div>
                  Dados: {shortPath(info.dataDir, home)} · Registros: {shortPath(info.logDir, home)}
                </div>
              </div>
            )
          }
          tiles={[
            {
              id: "removal",
              title: mode === "trash" ? "Itens removidos vão para a Lixeira" : "Itens removidos vão para a Quarentena",
              description:
                mode === "trash"
                  ? "Saem de vez só quando você esvazia a Lixeira do macOS. É o modo mais seguro."
                  : "São movidos para uma pasta com a data e um registro em CSV. Continuam ocupando espaço até você enviá-los à Lixeira.",
              visual: <BentoIcon size={112}><Trash2 /></BentoIcon>,
              actions: (
                <BentoButton primary onClick={open("removal")}>
                  Alterar
                </BentoButton>
              ),
            },
            {
              id: "permissions",
              title: `Acesso Total ao Disco ${accessLabel}`,
              description: "O app funciona sem ele, mas não enxerga pastas protegidas pelo macOS.",
              visual: <BentoIcon size={56}><ShieldCheck /></BentoIcon>,
              actions: (
                <>
                  <BentoButton onClick={() => recheck()} disabled={checking}>
                    Verificar
                  </BentoButton>
                  {access !== "granted" && (
                    <BentoButton primary onClick={() => api.openSystemSettings("fullDiskAccess")}>
                      Abrir Ajustes do Sistema
                    </BentoButton>
                  )}
                </>
              ),
            },
            {
              id: "menuBar",
              title: bar?.enabled ? "Barra de menus ligada" : "Barra de menus desligada",
              description: bar?.enabled ? `Ao lado do ícone: ${menuBarTitle[bar.title]}.` : "Mostre CPU, memória e disco sem abrir a janela.",
              visual: <BentoIcon size={48}><MenuSquare /></BentoIcon>,
              actions: (
                <BentoButton primary onClick={open("menuBar")}>
                  Ajustar
                </BentoButton>
              ),
            },
            {
              id: "ignored",
              title: `${ignored?.length ?? 0} ${ignored?.length === 1 ? "pasta ignorada" : "pastas ignoradas"}`,
              description: "Nenhuma análise lê essas pastas e elas nunca são sugeridas para remoção.",
              visual: <BentoIcon size={48}><EyeOff /></BentoIcon>,
              actions: (
                <BentoButton primary onClick={open("ignored")}>
                  Gerenciar
                </BentoButton>
              ),
            },
          ]}
        />
      </div>
      {panel && <PanelModal panel={panel} onClose={() => setPanel(null)} />}
    </div>
  );
}
