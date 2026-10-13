import { useEffect, useState } from "react";
import { ArrowUpCircle, Globe, RefreshCw } from "lucide-react";
import { api } from "../../lib/ipc";
import { useJob, useJobs } from "../../stores/jobs";
import { Badge, Button, Card, ErrorNote, PageHeader, ProgressBar } from "../../components/ui";
import { UpdatesLoader } from "../../components/loaders";
import { Modal } from "../../components/ReviewDialog";
import { formatCount } from "../../lib/format";
import { AppIcon } from "./shared";
import type { UpdateInfo, UpdatesResult } from "../../types";

const MODULE = "updates";

function Row({ u }: { u: UpdateInfo }) {
  return (
    <div className="grid grid-cols-[36px_minmax(0,1fr)_120px_120px_auto] items-center gap-3 px-4 py-2">
      <AppIcon path={u.path} />
      <div className="min-w-0">
        <div className="flex items-center gap-1.5">
          <span className="truncate font-medium">{u.name}</span>
          {u.source === "appStore" && <Badge>App Store</Badge>}
          {u.source === "sparkle" && <Badge>Atualizador do app</Badge>}
        </div>
        {u.error && <div className="truncate text-[11.5px] text-ink-3">Não foi possível verificar: {u.error}</div>}
      </div>
      <div className="text-[12.5px] text-ink-2">{u.installed ?? "—"}</div>
      <div className={`text-[12.5px] ${u.updateAvailable ? "font-semibold text-accent" : "text-ink-3"}`}>{u.latest ?? "—"}</div>
      <div className="w-[150px] text-right">
        {u.updateAvailable &&
          (u.source === "appStore" && u.url ? (
            <Button size="sm" onClick={() => api.openAppStorePage(u.url!)}>
              Abrir na App Store
            </Button>
          ) : (
            <Button size="sm" onClick={() => api.openApplication(u.path)}>
              Abrir o app
            </Button>
          ))}
      </div>
    </div>
  );
}

export function UpdatesPage() {
  const job = useJob<UpdatesResult>(MODULE);
  const start = useJobs((s) => s.start);
  const [ask, setAsk] = useState(false);
  const r = job.result;
  const running = job.status === "running";

  useEffect(() => {
    if (job.status === "idle") start(MODULE, "start_update_check", { online: false });
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const withSource = (r?.apps ?? []).filter((a) => a.source !== "none");
  const without = (r?.apps ?? []).filter((a) => a.source === "none");
  const updates = withSource.filter((a) => a.updateAvailable);

  return (
    <div className="h-full overflow-y-auto">
      <PageHeader
        title="Atualizações"
        subtitle="Versões instaladas e, se você pedir, as mais recentes da App Store e do atualizador de cada app. O OrganizeMyMac não baixa nem instala nada: ele abre o app ou a App Store para você atualizar por lá."
        actions={
          <Button variant="primary" icon={<Globe className="size-3.5" />} busy={running} onClick={() => setAsk(true)}>
            Verificar online…
          </Button>
        }
      />
      <div className="space-y-4 px-8 pb-10">
        {job.status === "failed" && <ErrorNote>{job.error}</ErrorNote>}
        {running && (
          <Card className="p-5">
            <UpdatesLoader />
            <div className="mb-3 text-[13px] font-medium">{job.stage ? `Consultando ${formatCount(job.stage.done)} de ${formatCount(job.stage.total)}` : "Lendo os apps instalados"}</div>
            <ProgressBar value={job.stage ? job.stage.done / Math.max(job.stage.total, 1) : undefined} indeterminate={!job.stage} />
          </Card>
        )}
        {r && !running && (
          <>
            {r.checkedOnline ? (
              <div className="flex items-center gap-2 text-[13px]">
                <ArrowUpCircle className="size-4 text-accent" />
                <span className="font-semibold">{formatCount(updates.length)}</span> {updates.length === 1 ? "atualização disponível" : "atualizações disponíveis"}
              </div>
            ) : (
              <div className="text-[12.5px] text-ink-2">
                {formatCount(withSource.length)} apps têm uma fonte de atualização conhecida. Clique em Verificar online para comparar as versões.
              </div>
            )}
            <Card className="divide-y divide-line">
              <div className="grid grid-cols-[36px_minmax(0,1fr)_120px_120px_auto] gap-3 px-4 py-1.5 text-[11px] font-medium text-ink-3">
                <span />
                <span>App</span>
                <span>Instalada</span>
                <span>Mais recente</span>
                <span className="w-[150px]" />
              </div>
              {withSource.map((u) => (
                <Row key={u.path} u={u} />
              ))}
            </Card>
            {without.length > 0 && (
              <details className="text-[12.5px] text-ink-2">
                <summary className="cursor-pointer">{formatCount(without.length)} apps sem fonte conhecida (atualizam por conta própria ou por instalador)</summary>
                <div className="mt-2 columns-3 text-[12px] text-ink-3">
                  {without.map((u) => (
                    <div key={u.path}>
                      {u.name} {u.installed && `· ${u.installed}`}
                    </div>
                  ))}
                </div>
              </details>
            )}
          </>
        )}
      </div>
      {ask && (
        <Modal onClose={() => setAsk(false)} width={480}>
          <div className="p-6">
            <h2 className="flex items-center gap-2 text-[16px] font-semibold">
              <RefreshCw className="size-4" /> Verificar atualizações online
            </h2>
            <p className="mt-1.5 text-ink-2">
              O app vai consultar a App Store (com o identificador de cada app da App Store) e o endereço de atualização que cada app declara
              (sempre HTTPS). É o mesmo pedido que os próprios apps fazem. Nenhum dado seu é enviado além disso.
            </p>
          </div>
          <div className="flex justify-end gap-2 border-t border-line px-6 py-3">
            <Button onClick={() => setAsk(false)}>Cancelar</Button>
            <Button
              variant="primary"
              onClick={() => {
                setAsk(false);
                start(MODULE, "start_update_check", { online: true });
              }}
            >
              Verificar
            </Button>
          </div>
        </Modal>
      )}
    </div>
  );
}
