import { useNav, type Section } from "../stores/nav";
import { useJobs } from "../stores/jobs";
import { useStage } from "../stores/stage";
import { appIcon, home, modules, settingsIcon, smartCare, tools, type Module } from "./modules";

// Rows sit on the window gradient; the active one becomes a glass pill.
// In the icon rail the row is a circle, so the active border is round.
const rowClass = (active: boolean, compact = false) =>
  `flex items-center text-left text-[13px] text-ink transition-colors duration-150 ${
    compact ? "mx-auto size-[46px] justify-center rounded-full" : "h-[38px] w-full gap-2.5 rounded-lg px-2"
  } ${active ? "glass font-semibold" : "font-medium border border-transparent hover:bg-white/10"}`;

function useRunning(jobs: (string | undefined)[]) {
  return useJobs((s) => jobs.some((j) => j && s.jobs[j]?.status === "running"));
}

// A finished analysis the user has not looked at yet: set while any job of the module is done.
function useReady(jobs: (string | undefined)[]) {
  return useJobs((s) => jobs.some((j) => j && s.jobs[j]?.status === "done"));
}

// Badge on a module icon: a spinning arc while it works, a dot when results are waiting.
function Badge({ running, ready }: { running: boolean; ready: boolean }) {
  if (running)
    return (
      <span className="absolute -top-1 -right-1.5 grid size-[14px] place-items-center rounded-full bg-white/90 shadow" aria-label="em andamento">
        <svg viewBox="0 0 20 20" className="size-[10px] animate-spin">
          <circle cx="10" cy="10" r="7" fill="none" stroke="rgb(0 0 0 / 0.2)" strokeWidth="3" />
          <path d="M10 3a7 7 0 0 1 7 7" fill="none" stroke="#1b1650" strokeWidth="3" strokeLinecap="round" />
        </svg>
      </span>
    );
  if (ready) return <span className="absolute -top-0.5 -right-1 size-[9px] rounded-full bg-white shadow ring-2 ring-[var(--deep,#1b1650)]/60" aria-label="resultado disponível" />;
  return null;
}

function Pulse() {
  return <span className="size-1.5 shrink-0 rounded-full bg-safe animate-pulse" aria-label="em andamento" />;
}

function SubItem({ id, label, job }: { id: Section; label: string; job?: string }) {
  const { section, go } = useNav();
  const running = useRunning([job]);
  const active = section === id;
  return (
    <button
      onClick={() => go(id)}
      className={`flex h-7 w-full items-center gap-2 rounded-md pr-2 pl-[50px] text-left text-[12.5px] transition-colors duration-150 ${
        active ? "bg-white/15 font-semibold text-ink" : "text-ink-2 hover:bg-white/8 hover:text-ink"
      }`}
    >
      <span className="flex-1 truncate">{label}</span>
      {running && <Pulse />}
    </button>
  );
}

function ModuleItem({ module, compact }: { module: Module; compact: boolean }) {
  const { section, go } = useNav();
  const running = useRunning(module.sections.map((s) => s.job));
  const ready = useReady(module.sections.map((s) => s.job)) && !module.sections.some((s) => s.id === section);
  const inside = module.sections.some((s) => s.id === section);
  const expanded = inside && module.sections.length > 1 && !compact;
  return (
    <div>
      <button
        onClick={() => go(module.sections[0].id)}
        title={compact ? module.label : undefined}
        className={`${rowClass(inside, compact)} relative`}
      >
        <span className="relative shrink-0">
          <img src={module.icon} alt="" className="size-[30px] object-contain" draggable={false} />
          {!expanded && <Badge running={running} ready={ready} />}
        </span>
        {!compact && <span className="flex-1 truncate">{module.label}</span>}
        {!compact && module.badge && <small className="rounded bg-white/18 px-1.5 py-px text-[9px] font-medium text-ink-2">{module.badge}</small>}
      </button>
      {expanded && (
        <div className="mt-[3px] space-y-px">
          {module.sections.map((s) => (
            <SubItem key={s.id} {...s} />
          ))}
        </div>
      )}
    </div>
  );
}

function Group({ label, items, compact }: { label?: string; items: Module[]; compact: boolean }) {
  return (
    <div>
      {label && (compact ? <div className="mx-2 mt-3 mb-3 border-t border-white/12" /> : <div className="mx-2 mt-[19px] mb-1.5 text-[11px] font-bold text-ink-2">{label}</div>)}
      <div className={compact ? "space-y-2" : "space-y-[3px]"}>
        {items.map((m) => (
          <ModuleItem key={m.id} module={m} compact={compact} />
        ))}
      </div>
    </div>
  );
}

export function Sidebar() {
  const { section, go } = useNav();
  const compact = useStage((s) => s.compact);
  return (
    <nav
      className={`drag flex h-full shrink-0 flex-col pt-[52px] pb-3.5 transition-[width,padding] duration-300 [text-shadow:0_1px_3px_rgb(0_0_0/0.3)] ${compact ? "w-[76px] px-3" : "w-[212px] px-3"}`}
    >
      <div className={`flex items-center gap-2.5 pb-5 text-[15px] font-semibold text-ink ${compact ? "justify-center" : "px-2"}`}>
        <img src={appIcon} alt="" className="size-[30px] object-contain" draggable={false} />
        {!compact && "OrganizeMyMac"}
      </div>
      <div className="-mx-1 flex-1 overflow-y-auto px-1 [-webkit-app-region:no-drag]">
        <Group items={[home, smartCare]} compact={compact} />
        <Group label="Módulos" items={modules} compact={compact} />
        <Group label="Ferramentas" items={tools} compact={compact} />
      </div>
      <div className="pt-3 [-webkit-app-region:no-drag]">
        <button onClick={() => go("settings")} title={compact ? "Ajustes" : undefined} className={rowClass(section === "settings", compact)}>
          <img src={settingsIcon} alt="" className="size-[30px] shrink-0 object-contain" draggable={false} />
          {!compact && <span className="flex-1 truncate">Ajustes</span>}
        </button>
      </div>
    </nav>
  );
}
