// Development-only preview mode: when the UI runs in a plain browser (`npm run dev`), commands are
// answered with fictitious data so screens can be designed and checked without the Rust core.
// Never bundled into the app (imported only when `import.meta.env.DEV` and outside Tauri).
import { mockIPC } from "@tauri-apps/api/mocks";
import type { HomebrewReport, Report, QuarantineList, QuarantineView, RemovalMode, ExclusionPreview, TmExclusion, BackupAck, BackupGate, BackupState, DevArtifact, DevReport, DevRoot, PushPlan, RemoteView, RepoReport, SecretFinding, SnapshotPlan, SnapshotRecord, Blocker, SecurityAudit, CloudUsage, UpdatesResult, RulesReport, SmartCareReport, SimilarResult, AppInfo, AppsResult, Leftover, OrphansResult, UninstallPlan, DuplicateGroup, DuplicatesResult, DownloadItem, DownloadsResult, FileCategory, FileEntry, FindResult, ScanResult, StorageNode } from "../types";

const HOME = "/Users/demo";
const now = Date.now();
const DAY = 86_400_000;

type Handler = (args: Record<string, unknown>) => unknown;
const handlers: Record<string, Handler> = {};
export function registerMock(command: string, handler: Handler) {
  handlers[command] = handler;
}

/** Streams job events to a Channel like the Rust side does. */
export function streamJob<T>(channelArg: unknown, root: string, result: () => T, durationMs = 1800) {
  const id = (channelArg as { id: number }).id;
  let index = 0;
  const send = (message: unknown) =>
    (window as unknown as { __TAURI_INTERNALS__: { runCallback: (id: number, d: unknown) => void } }).__TAURI_INTERNALS__.runCallback(id, {
      index: index++,
      message,
    });
  const jobId = crypto.randomUUID();
  setTimeout(() => send({ event: "start", jobId, root }), 10);
  const steps = 12;
  for (let i = 1; i <= steps; i++) {
    setTimeout(() => {
      const f = i / steps;
      send({
        event: "progress",
        files: Math.round(412_000 * f),
        directories: Math.round(61_000 * f),
        bytesScanned: Math.round(318e9 * f),
        current: `${root}/Library/Developer/CoreSimulator/Devices/${i}`,
        elapsedMs: Math.round(durationMs * f),
        warnings: i > 6 ? 2 : 0,
        waiting: false,
      });
      if (i === 7) {
        send({ event: "warning", path: `${HOME}/Library/Containers`, message: "not scanned: needs Full Disk Access" });
        send({ event: "warning", path: `${HOME}/Library/CloudStorage`, message: "not scanned: cloud storage" });
      }
    }, (durationMs / steps) * i);
  }
  setTimeout(() => {
    send({ event: "complete", result: result() });
    send({ end: true, index });
  }, durationMs + 50);
  return jobId;
}

export function fakeFile(path: string, size: number, ageDays: number, category: FileCategory = "other"): FileEntry {
  const name = path.split("/").pop() ?? path;
  const ext = name.includes(".") ? name.split(".").pop()!.toLowerCase() : null;
  return {
    path,
    name,
    sizeLogical: size,
    sizeAllocated: size,
    createdAt: now - (ageDays + 3) * DAY,
    modifiedAt: now - ageDays * DAY,
    extension: ext,
    category,
    isDirectory: false,
    isSymlink: false,
    isCloudPlaceholder: false,
  };
}

let nodeId = 1;
function node(path: string, size: number, children?: StorageNode[], isDir = true): StorageNode {
  return {
    id: nodeId++,
    name: path.split("/").pop() || path,
    path,
    size,
    allocated: Math.round(size * 0.96),
    files: Math.round(size / 900_000),
    isDir,
    looseFilesSize: children ? Math.max(0, size - children.reduce((s, c) => s + c.size, 0)) : 0,
    hasChildren: isDir,
    children: children ?? null,
  };
}

export const demoTree = (): StorageNode =>
  node(HOME, 318e9, [
    node(`${HOME}/Library`, 124e9),
    node(`${HOME}/Movies`, 61e9),
    node(`${HOME}/Projects`, 48e9),
    node(`${HOME}/Pictures`, 33e9),
    node(`${HOME}/Downloads`, 21.4e9),
    node(`${HOME}/Documents`, 14.2e9),
    node(`${HOME}/Music`, 7.9e9),
    node(`${HOME}/Desktop`, 3.1e9),
    node(`${HOME}/.docker`, 2.6e9),
    node(`${HOME}/.npm`, 1.4e9),
    node(`${HOME}/Parallels.pvm`, 900e6, undefined, false),
  ]);

export const demoLargest = (): FileEntry[] => [
  fakeFile(`${HOME}/Movies/Viagem Patagônia 4K.mov`, 18.4e9, 420, "video"),
  fakeFile(`${HOME}/Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw`, 14.1e9, 2, "diskImage"),
  fakeFile(`${HOME}/Downloads/Xcode_26.6.xip`, 11.2e9, 95, "archive"),
  fakeFile(`${HOME}/Movies/Casamento — edição final.mp4`, 7.8e9, 610, "video"),
  fakeFile(`${HOME}/Documents/Backup iPhone 2024.zip`, 6.3e9, 520, "archive"),
  fakeFile(`${HOME}/Downloads/ubuntu-24.04-desktop-arm64.iso`, 3.4e9, 300, "diskImage"),
  fakeFile(`${HOME}/Projects/ml-sandbox/models/llama-8b-q4.gguf`, 4.9e9, 140, "other"),
  fakeFile(`${HOME}/Downloads/Figma-126.1.dmg`, 540e6, 60, "diskImage"),
  fakeFile(`${HOME}/Pictures/Export Lightroom/panorama-final.tif`, 1.9e9, 800, "image"),
  fakeFile(`${HOME}/Music/Logic/Sessão 12/Audio Files/take-03.wav`, 1.2e9, 900, "audio"),
];

registerMock("start_scan", ({ root, onEvent }) =>
  streamJob<ScanResult>(onEvent, root as string, () => ({
    id: "demo-scan",
    root: root as string,
    startedAt: now - 1800,
    finishedAt: now,
    files: 412_318,
    directories: 61_204,
    bytesScanned: 318e9,
    bytesAllocated: 301e9,
    symlinks: 1_830,
    cloudPlaceholders: 214,
    warnings: 2,
    durationMs: 1790,
    cancelled: false,
    reclaimableBytes: 0,
    largestFiles: demoLargest(),
    tree: demoTree(),
  })),
);
registerMock("cancel_job", () => true);
registerMock("permission_status", () => ({ fullDiskAccess: "denied", home: HOME }));
registerMock("open_system_settings", () => null);
registerMock("suggested_locations", () =>
  ["home", "downloads", "documents", "desktop", "applications", "disk"].map((id) => ({
    id,
    exists: true,
    path: { home: HOME, downloads: `${HOME}/Downloads`, documents: `${HOME}/Documents`, desktop: `${HOME}/Desktop`, applications: "/Applications", disk: "/" }[id],
  })),
);
registerMock("app_info", () => ({ version: "0.1.0", dataDir: `${HOME}/Library/Application Support/dev.galuppo.OrganizeMyMac`, logDir: `${HOME}/Library/Logs/dev.galuppo.OrganizeMyMac` }));
registerMock("recent_scans", () => []);
registerMock("operation_log", () => [
  { id: 2, ts: now - 3_600_000, action: "trash", path: `${HOME}/Downloads/Figma-125.dmg`, size: 530e6, ok: true },
  { id: 1, ts: now - 7_200_000, action: "trash", path: `${HOME}/Downloads/old-installer.pkg`, size: 210e6, ok: true },
]);
registerMock("local_metrics", () => ({ scans_total: 14, files_scanned_total: 5_120_004, bytes_reclaimed_total: 23.4e9, items_removed_total: 187, operation_errors_total: 1 }));
registerMock("ignore_list", () => [{ path: `${HOME}/Projects/keep-forever`, addedAt: now - 30 * DAY, reason: null }]);
registerMock("add_to_ignore_list", () => null);
registerMock("remove_from_ignore_list", () => null);
registerMock("plugin:dialog|open", () => `${HOME}/Projects`);

const baseScan = (root: string): ScanResult => ({
  id: crypto.randomUUID(),
  root,
  startedAt: now - 1500,
  finishedAt: now,
  files: 412_318,
  directories: 61_204,
  bytesScanned: 318e9,
  bytesAllocated: 301e9,
  symlinks: 1_830,
  cloudPlaceholders: 214,
  warnings: 0,
  durationMs: 1490,
  cancelled: false,
  reclaimableBytes: 0,
  largestFiles: [],
  tree: null,
});

registerMock("disk_overview", () => [
  { name: "Macintosh HD", mountPoint: "/", fileSystem: "apfs", total: 994.66e9, free: 212.4e9, used: 782.2e9, isRoot: true, readOnly: false, local: true },
  { name: "Backup", mountPoint: "/Volumes/Backup", fileSystem: "apfs", total: 2e12, free: 1.31e12, used: 690e9, isRoot: false, readOnly: false, local: true },
]);

registerMock("start_find_files", ({ root, filter, onEvent }) =>
  streamJob<FindResult>(onEvent, root as string, () => {
    const f = filter as { minSize: number };
    const matches = demoLargest().filter((x) => x.sizeLogical >= f.minSize && !x.path.includes("/Library/"));
    return { ...baseScan(root as string), matches, matched: matches.length, matchedBytes: matches.reduce((a, b) => a + b.sizeLogical, 0), truncated: false };
  }, 1400),
);

const dl = (name: string, size: number, age: number, category: FileCategory, group: DownloadItem["group"], extra: Partial<DownloadItem> = {}): DownloadItem => ({
  ...fakeFile(`${HOME}/Downloads/${name}`, size, age, category),
  group,
  ageDays: age,
  extracted: false,
  confidence: group === "oldInstaller" ? "safe" : "review",
  selected: group === "oldInstaller",
  ...extra,
});

registerMock("start_downloads_scan", ({ onEvent }) =>
  streamJob<DownloadsResult>(onEvent, `${HOME}/Downloads`, () => {
    const items: DownloadItem[] = [
      dl("Figma-126.1.dmg", 540e6, 60, "diskImage", "oldInstaller"),
      dl("Docker.dmg", 610e6, 120, "diskImage", "oldInstaller"),
      dl("Zoom.pkg", 98e6, 210, "installer", "oldInstaller"),
      dl("GoogleChrome.dmg", 230e6, 45, "diskImage", "oldInstaller"),
      dl("Xcode_26.6.xip", 11.2e9, 95, "other", "largeFile"),
      dl("ubuntu-24.04-desktop-arm64.iso", 3.4e9, 300, "diskImage", "largeFile"),
      dl("fotos-viagem.zip", 1.2e9, 40, "archive", "largeFile"),
      dl("brand-assets.zip", 86e6, 22, "archive", "archive", { extracted: true, confidence: "safe" }),
      dl("relatorio-dados.tar.gz", 12e6, 16, "archive", "archive"),
      dl("Raycast.dmg", 72e6, 4, "diskImage", "installer"),
      dl("Captura de Tela 2026-09-12 às 10.41.22.png", 1.8e6, 16, "image", "screenshot"),
      dl("Captura de Tela 2026-09-20 às 18.02.03.png", 2.3e6, 8, "image", "screenshot"),
      dl("contrato-2025.pdf", 1.1e6, 290, "document", "oldFile"),
      dl("apresentacao.key", 44e6, 12, "document", "recent"),
    ];
    return { ...baseScan(`${HOME}/Downloads`), folder: `${HOME}/Downloads`, items, totalBytes: items.reduce((a, b) => a + b.sizeLogical, 0) };
  }, 1200),
);

registerMock("trash_summary", () => ({
  path: `${HOME}/.Trash`,
  readable: true,
  files: 1_204,
  folders: 88,
  bytes: 6.1e9,
  items: [
    { ...fakeFile(`${HOME}/.Trash/old-project`, 3.9e9, 30), isDirectory: true },
    fakeFile(`${HOME}/.Trash/Figma-125.dmg`, 530e6, 60, "diskImage"),
    fakeFile(`${HOME}/.Trash/gravação.mov`, 1.4e9, 10, "video"),
    fakeFile(`${HOME}/.Trash/notas.txt`, 12e3, 5, "document"),
  ],
}));
registerMock("empty_trash", () => [{ path: `${HOME}/.Trash/old-project`, size: 6.1e9, ok: true }]);
registerMock("empty_trash_with_finder", () => null);
registerMock("reveal_in_finder", () => null);
registerMock("quick_look", () => null);
registerMock("path_exists", () => true);
registerMock("move_to_trash", ({ request }) =>
  (request as { items: { path: string; size: number }[] }).items.map((i) => ({ ...i, ok: !i.path.includes("/Library/"), error: i.path.includes("/Library/") ? "blocked by safety rule" : null })),
);

const dupGroup = (name: string, size: number, paths: string[], category: FileCategory): DuplicateGroup => ({
  hash: crypto.randomUUID().replace(/-/g, ""),
  size,
  wasted: size * (paths.length - 1),
  files: paths.map((p, i) => ({ ...fakeFile(`${HOME}/${p}/${name}`, size, 200 - i * 30, category), hardLinks: [], selected: i > 0 })),
});

registerMock("start_duplicate_scan", ({ root, onEvent }) =>
  streamJob<DuplicatesResult>(onEvent, root as string, () => {
    const groups = [
      dupGroup("Viagem Patagônia 4K.mov", 18.4e9, ["Movies", "Desktop/Backup vídeos"], "video"),
      dupGroup("Backup iPhone 2024.zip", 6.3e9, ["Documents", "Downloads"], "archive"),
      dupGroup("ubuntu-24.04-desktop-arm64.iso", 3.4e9, ["Downloads", "Projects/vm-images", "Desktop"], "diskImage"),
      dupGroup("IMG_4021.HEIC", 3.1e6, ["Pictures/2024", "Desktop/fotos", "Downloads"], "image"),
      dupGroup("contrato-assinado.pdf", 1.2e6, ["Documents/Contratos", "Downloads"], "document"),
    ];
    return {
      ...baseScan(root as string),
      groups,
      totalGroups: groups.length,
      wastedBytes: groups.reduce((s, g) => s + g.wasted, 0),
      hashStats: { candidates: 8307, afterSize: 2890, sampleHashed: 2890, fullHashed: 1616, bytesHashed: 8.9e9, hashMs: 4940 },
    };
  }, 1600),
);
registerMock("move_duplicates_to_trash", ({ groups }) =>
  (groups as { remove: { path: string; size: number }[] }[]).flatMap((g) => g.remove.map((r) => ({ path: r.path, size: r.size, ok: true, error: null }))),
);

function nested(path: string, size: number, depth: number): StorageNode {
  const names = ["Library", "Developer", "Caches", "Application Support", "Movies", "Projetos", "node_modules", "Fotos", "Backups", "Xcode", "DerivedData", "Docker", "Music", "Arquivos"];
  let seed = [...path].reduce((a, c) => (a * 31 + c.charCodeAt(0)) % 9973, 7);
  const rnd = () => ((seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648);
  const count = depth > 0 ? 6 + Math.floor(rnd() * 8) : 0;
  let left = size * 0.93;
  const children: StorageNode[] = [];
  for (let i = 0; i < count && left > 1e6; i++) {
    const share = i === count - 1 ? left : left * (0.18 + rnd() * 0.35);
    left -= share;
    const isDir = rnd() > 0.2;
    const name = isDir ? `${names[Math.floor(rnd() * names.length)]} ${i + 1}` : `arquivo-${i + 1}.${["mov", "zip", "dmg", "raw"][i % 4]}`;
    children.push({ ...nested(`${path}/${name}`, share, isDir ? depth - 1 : 0), isDir, hasChildren: isDir });
  }
  return {
    id: nodeId++,
    name: path.split("/").pop() || path,
    path,
    size,
    allocated: size,
    files: Math.round(size / 2e6),
    isDir: true,
    looseFilesSize: size - children.reduce((s, c) => s + c.size, 0),
    hasChildren: children.length > 0,
    children: depth > 0 ? children : null,
  };
}
const nodeSizes = new Map<string, number>([[HOME, 318e9]]);
registerMock("storage_node", ({ path, depth }) => {
  const p = (path as string | undefined) ?? HOME;
  const n = nested(p, nodeSizes.get(p) ?? 40e9, (depth as number) ?? 1);
  const remember = (x: StorageNode) => {
    nodeSizes.set(x.path, x.size);
    x.children?.forEach(remember);
  };
  remember(n);
  return n;
});

const app = (name: string, id: string, size: number, lastUsedDays: number | null, extra: Partial<AppInfo> = {}): AppInfo => ({
  path: `/Applications/${name}.app`,
  name,
  bundleId: id,
  version: "1." + (name.length % 9) + ".2",
  build: "100",
  minimumSystem: "13.0",
  size,
  modifiedAt: now - 40 * DAY,
  lastUsedAt: lastUsedDays == null ? null : now - lastUsedDays * DAY,
  fromAppStore: false,
  systemApp: false,
  running: false,
  iconFile: null,
  ...extra,
});
const demoApps = [
  app("Xcode", "com.apple.dt.Xcode", 9.45e9, 1, { fromAppStore: true, running: true }),
  app("Android Studio", "com.google.android.studio", 2.91e9, 240),
  app("Docker", "com.docker.docker", 2.56e9, 12),
  app("Figma", "com.figma.Desktop", 612e6, 3),
  app("Google Chrome", "com.google.Chrome", 1.5e9, 0, { running: true }),
  app("Visual Studio Code", "com.microsoft.VSCode", 1.46e9, 0),
  app("Zoom", "us.zoom.xos", 382e6, 410),
  app("GarageBand", "com.apple.garageband10", 1.1e9, 900, { fromAppStore: true }),
  app("Spotify", "com.spotify.client", 410e6, 30),
  app("Safari", "com.apple.Safari", 18e6, 0, { systemApp: true }),
];
registerMock("start_app_list", ({ onEvent }) => streamJob<AppsResult>(onEvent, "/Applications", () => ({ apps: demoApps, totalSize: demoApps.reduce((s, a) => s + a.size, 0) }), 900));
registerMock("app_icon", () => null);
const lf = (path: string, kind: Leftover["kind"], size: number, rule: Leftover["rule"], confidence: Leftover["confidence"]): Leftover => ({
  path: `${HOME}/Library/${path}`,
  kind,
  size,
  rule,
  confidence,
  selected: confidence === "safe",
});
registerMock("uninstall_plan", ({ path }): UninstallPlan => {
  const a = demoApps.find((x) => x.path === path)!;
  return {
    appPath: a.path,
    name: a.name,
    bundleId: a.bundleId,
    appSize: a.size,
    running: a.running,
    protected: a.systemApp,
    fullDiskAccess: false,
    leftovers: [
      lf(`Application Support/${a.name}`, "applicationSupport", 250e6, "knownPath", "safe"),
      lf(`Caches/${a.bundleId}`, "caches", 94e6, "bundleId", "safe"),
      lf(`Preferences/${a.bundleId}.plist`, "preferences", 80e3, "bundleId", "safe"),
      lf(`Logs/${a.name}`, "logs", 18e6, "appName", "review"),
      lf(`Saved Application State/${a.bundleId}.savedState`, "savedState", 1.2e6, "bundleId", "safe"),
      lf(`Group Containers/ABCDE12345.${a.bundleId?.split(".").slice(0, 2).join(".")}.shared`, "groupContainer", 40e6, "sharedGroup", "danger"),
    ],
  };
});
registerMock("uninstall_app", ({ path, leftovers }) => [
  { path, size: 612e6, ok: true, error: null },
  ...(leftovers as string[]).map((p) => ({ path: p, size: 10e6, ok: true, error: null })),
]);
registerMock("start_orphan_scan", ({ onEvent }) =>
  streamJob<OrphansResult>(onEvent, `${HOME}/Library`, () => {
    const groups = [
      { bundleId: "com.tinyspeck.slackmacgap", vendorInstalled: false, items: [lf("Application Support/com.tinyspeck.slackmacgap", "applicationSupport", 275e6, "orphan", "review"), lf("Caches/com.tinyspeck.slackmacgap", "caches", 88e6, "orphan", "safe"), lf("Preferences/com.tinyspeck.slackmacgap.plist", "preferences", 12e3, "orphan", "safe")], size: 363e6 },
      { bundleId: "com.adobe.dunamis", vendorInstalled: true, items: [lf("Caches/com.adobe.dunamis", "caches", 104e6, "orphan", "review")], size: 104e6 },
      { bundleId: "com.duckduckgo.macos.browser", vendorInstalled: false, items: [lf("HTTPStorages/com.duckduckgo.macos.browser", "httpStorage", 20e6, "orphan", "safe"), lf("Preferences/com.duckduckgo.macos.browser.plist", "preferences", 40e3, "orphan", "safe")], size: 20e6 },
    ];
    return { groups, totalSize: groups.reduce((s, g) => s + g.size, 0), fullDiskAccess: false };
  }, 900),
);
registerMock("remove_orphans", ({ items }) => (items as { path: string; size: number }[]).map((i) => ({ ...i, ok: true, error: null })));

let menuBar = { enabled: true, title: "cpu" };
registerMock("menu_bar_settings", () => menuBar);
registerMock("set_menu_bar_settings", ({ settings }) => {
  menuBar = settings as typeof menuBar;
  return null;
});
registerMock("health", () => ({
  cpu: 8 + Math.random() * 22,
  cores: 10,
  loadAverage: [2.41, 2.2, 2.05],
  memoryTotal: 24e9,
  memoryUsed: 16.8e9 + Math.random() * 0.4e9,
  memoryAvailable: 7e9,
  swapTotal: 3.2e9,
  swapUsed: 1.1e9,
  diskTotal: 994.66e9,
  diskFree: 212.4e9,
  uptime: 3 * 86400 + 5 * 3600 + 12 * 60,
  battery: { percent: 87, charging: false, onAc: false, minutesRemaining: 312, state: "discharging" },
}));
registerMock("process_list", () => {
  const apps = [
    { name: "Google Chrome", appPath: "/Applications/Google Chrome.app", memory: 3.9e9, cpu: 12.4, processes: 23, canQuit: true },
    { name: "Xcode", appPath: "/Applications/Xcode.app", memory: 2.7e9, cpu: 4.1, processes: 6, canQuit: true },
    { name: "Visual Studio Code", appPath: "/Applications/Visual Studio Code.app", memory: 1.6e9, cpu: 2.3, processes: 11, canQuit: true },
    { name: "Figma", appPath: "/Applications/Figma.app", memory: 980e6, cpu: 1.1, processes: 5, canQuit: true },
    { name: "Spotify", appPath: "/Applications/Spotify.app", memory: 410e6, cpu: 0.6, processes: 4, canQuit: true },
  ];
  const processes = [
    ...apps.map((a, i) => ({ pid: 500 + i, name: a.name, memory: a.memory, cpu: a.cpu, category: "application", appPath: a.appPath, user: "demo" })),
    { pid: 900, name: "node", memory: 620e6, cpu: 3.2, category: "background", appPath: null, user: "demo" },
    { pid: 901, name: "rust-analyzer", memory: 1.1e9, cpu: 0.4, category: "background", appPath: null, user: "demo" },
    { pid: 1, name: "launchd", memory: 30e6, cpu: 0.1, category: "system", appPath: null, user: "root" },
    { pid: 120, name: "WindowServer", memory: 890e6, cpu: 6.2, category: "system", appPath: null, user: "_windowserver" },
    { pid: 140, name: "mds_stores", memory: 210e6, cpu: 1.4, category: "system", appPath: null, user: "root" },
  ];
  return { processes, apps };
});
registerMock("quit_application", () => 1);

const PICS = ["1e3a8a,3b82f6", "7c2d12,f97316", "14532d,22c55e", "581c87,a855f7"];
const svgThumb = (i: number, v: number) => {
  const [a, b] = PICS[i % PICS.length].split(",");
  const svg = `<svg xmlns='http://www.w3.org/2000/svg' width='320' height='240'><defs><linearGradient id='g' x1='0' y1='0' x2='1' y2='1'><stop offset='0' stop-color='#${a}'/><stop offset='1' stop-color='#${b}'/></linearGradient></defs><rect width='320' height='240' fill='url(#g)'/><circle cx='${210 + v * 6}' cy='${90 - v * 4}' r='${38 - v * 3}' fill='rgba(255,255,255,.75)'/><path d='M0 200 L90 120 L160 180 L230 110 L320 190 L320 240 L0 240Z' fill='rgba(0,0,0,.35)'/></svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
};
registerMock("image_thumbnail", ({ path }) => {
  const m = /g(\d)-(\d)/.exec(path as string);
  if (m) return svgThumb(+m[1], +m[2]);
  return /\.(heic|png|jpe?g)$/i.test(path as string) ? svgThumb(2, 1) : null;
});
registerMock("photos_status", () => "notDetermined");
registerMock("start_similar_images", ({ root, onEvent }) =>
  streamJob<SimilarResult>(onEvent, root as string, () => {
    const groups = [0, 1, 2].map((gi) => {
      const images = [0, 1, 2, 3].slice(0, gi === 1 ? 2 : gi === 2 ? 4 : 3).map((v) => ({
        ...fakeFile(`${HOME}/Pictures/Viagem/g${gi}-${v} IMG_${4020 + gi * 10 + v}.jpg`, [6.2e6, 2.1e6, 890e3, 4.4e6][v], 300 - v, "image"),
        width: [4032, 2016, 1200, 4032][v],
        height: [3024, 1512, 900, 3024][v],
        distance: v === 0 ? 0.08 : 0.1 + v * 0.05,
        keep: v === 0,
      }));
      return { id: images[0].path, images, reclaimable: images.filter((x) => !x.keep).reduce((s, x) => s + x.sizeLogical, 0) };
    });
    return { ...baseScan(root as string), groups, images: 1840, truncated: false, vision: { analyzed: 1840, candidates: 212, featurePrints: 212, failed: 3, elapsedMs: 9400 }, reclaimable: groups.reduce((s, g) => s + g.reclaimable, 0) };
  }, 1500),
);

registerMock("start_smart_care", ({ onEvent }) => {
  const id = (onEvent as { id: number }).id;
  let index = 0;
  const send = (message: unknown) =>
    (window as unknown as { __TAURI_INTERNALS__: { runCallback: (id: number, d: unknown) => void } }).__TAURI_INTERNALS__.runCallback(id, { index: index++, message });
  const stages = ["trash", "downloads", "home", "duplicates", "apps", "leftovers", "projects"];
  stages.forEach((s, i) => setTimeout(() => send({ event: "stage", stage: s, done: i, total: 7 }), 250 * i));
  setTimeout(() => {
    const dl = (name: string, size: number, age: number, sel: boolean): DownloadItem => ({ ...fakeFile(`${HOME}/Downloads/${name}`, size, age, "diskImage"), group: sel ? "oldInstaller" : "recent", ageDays: age, extracted: false, confidence: sel ? "safe" : "review", selected: sel });
    const result: SmartCareReport = {
      diskTotal: 994.66e9,
      diskFree: 82e9,
      trash: { path: `${HOME}/.Trash`, readable: true, files: 412, folders: 20, bytes: 2.5e9, items: [fakeFile(`${HOME}/.Trash/old-project.zip`, 2.5e9, 12, "archive")] },
      downloads: [dl("Figma-126.1.dmg", 540e6, 60, true), dl("Docker.dmg", 610e6, 120, true), dl("Zoom.pkg", 98e6, 210, true), dl("apresentacao.key", 44e6, 12, false)],
      duplicates: [dupGroup("Viagem Patagônia 4K.mov", 1.4e9, ["Movies", "Desktop/Backup vídeos"], "video"), dupGroup("Backup iPhone 2024.zip", 1.7e9, ["Documents", "Downloads"], "archive")],
      largeOld: [fakeFile(`${HOME}/Documents/VM/Windows 11.vhdx`, 5.2e9, 700, "diskImage")],
      unusedApps: demoApps.filter((a) => (a.lastUsedAt ?? 0) < Date.now() - 180 * DAY),
      leftovers: [{ bundleId: "com.tinyspeck.slackmacgap", vendorInstalled: false, items: [lf("Caches/com.tinyspeck.slackmacgap", "caches", 88e6, "orphan", "safe")], size: 88e6 }],
      dev: devReport(),
      recommendations: [
        { id: "lowDisk", level: "high", title: "Pouco espaço livre", detail: "Restam 82,0 GB (8% do disco). Abaixo de 10% o macOS pode ficar lento e falhar em atualizações.", bytes: 0 },
        { id: "devUnpushed", level: "suggest", title: "Trabalho não enviado em 6 repositórios", detail: "Parte do que está lá só existe neste Mac. Faça um snapshot em Projetos antes de limpar.", bytes: 0 },
        { id: "devArtifacts", level: "suggest", title: "Pastas dos projetos que dá para recriar", detail: "node_modules, target e similares somam 3,1 GB e voltam com um comando.", bytes: 3.1e9 },
        { id: "duplicates", level: "suggest", title: "Remover cópias duplicadas", detail: "Arquivos idênticos de mais de 10 MB ocupam 3,1 GB a mais.", bytes: 3.1e9 },
        { id: "emptyTrash", level: "suggest", title: "Esvaziar a Lixeira", detail: "A Lixeira ocupa 2,5 GB.", bytes: 2.5e9 },
        { id: "installers", level: "suggest", title: "Revisar instaladores em Downloads", detail: "3 instaladores; os antigos somam 1,2 GB.", bytes: 1.2e9 },
      ],
      durationMs: 15300,
    };
    send({ event: "complete", result });
    send({ end: true, index });
  }, 1700);
  return crypto.randomUUID();
});
registerMock("run_smart_care", ({ plan }) => {
  const p = plan as { files: { path: string; size: number }[]; leftovers: { path: string; size: number }[]; artifacts?: { path: string; size: number }[] };
  p.artifacts?.forEach((a) => removedArtifacts.add(a.path));
  return [...p.files, ...p.leftovers, ...(p.artifacts ?? [])].map((i) => ({ path: i.path, size: i.size, ok: true, error: null }));
});

const rule = (id: string, group: string, title: string, description: string, risk: string, items: [string, number, string | null, boolean][], extra: Record<string, unknown> = {}) => ({
  id, group, title, description, paths: [], risk, requiresClosed: [], app: null, perItemRunningCheck: false, needsFullDiskAccess: false, selected: risk === "low",
  items: items.map(([p, size, blocked, system]) => ({ path: `${HOME}/${p}`, size, blocked, system, selected: risk === "low" && !blocked && !system && group !== "browser" })),
  size: items.reduce((s, i) => s + i[1], 0), blockedBy: [], unavailable: false, ...extra,
});
registerMock("cleanup_rules", (): RulesReport => ({
  fullDiskAccess: false,
  custom: [{ id: "c1", title: "Exportações do Figma", folder: "Projects/exports", risk: "medium" }],
  results: [
    rule("user-caches", "user", "Caches de apps", "Arquivos temporários que os apps recriam quando precisam. Os caches de apps abertos ficam de fora.", "low", [
      ["Library/Caches/com.spotify.client", 1.2e9, null, false], ["Library/Caches/Google", 960e6, "Google Chrome", false], ["Library/Caches/Homebrew", 740e6, null, false], ["Library/Caches/com.apple.Music", 120e6, null, true],
    ]),
    rule("user-logs", "user", "Registros (logs) de apps", "Arquivos de registro gravados pelos apps.", "low", [["Library/Logs/DiagnosticReports", 180e6, null, false], ["Library/Logs/Zoom", 60e6, null, false]]),
    rule("xcode-derived-data", "developer", "Xcode: DerivedData", "Builds intermediários e índices. O Xcode reconstrói no próximo build.", "low", [["Library/Developer/Xcode/DerivedData/App-abc", 1.08e9, "Xcode", false]], { blockedBy: ["Xcode"] }),
    rule("npm-cache", "developer", "npm: cache", "Pacotes baixados pelo npm. São baixados de novo quando necessário.", "low", [[".npm/_cacache", 16.6e9, null, false]]),
    rule("chrome-history", "browser", "Chrome: histórico", "Histórico de navegação e de downloads de cada perfil.", "medium", [["Library/Application Support/Google/Chrome/Default/History", 80e6, null, false]]),
    rule("mail-downloads", "mail", "Mail: anexos abertos", "Cópias de anexos que você abriu no Mail.", "low", [], { unavailable: true }),
    rule("custom:c1", "custom", "Exportações do Figma", "", "medium", [["Projects/exports/v1", 420e6, null, false], ["Projects/exports/v2", 380e6, null, false]]),
  ],
} as unknown as RulesReport));
registerMock("run_cleanup_rules", ({ items }) => (items as { path: string; size: number }[]).map((i) => ({ path: i.path, size: i.size, ok: true, error: null })));
registerMock("add_custom_rule", () => null);
registerMock("remove_custom_rule", () => null);

registerMock("cloud_folders", () => [
  { provider: "iCloud", name: "iCloud Drive", path: `${HOME}/Library/Mobile Documents/com~apple~CloudDocs`, canEvict: true },
  { provider: "googleDrive", name: "GoogleDrive · demo@gmail.com", path: `${HOME}/Library/CloudStorage/GoogleDrive-demo@gmail.com`, canEvict: false },
  { provider: "dropbox", name: "Dropbox", path: `${HOME}/Library/CloudStorage/Dropbox`, canEvict: false },
]);
registerMock("start_cloud_usage", ({ path, onEvent }) =>
  streamJob<CloudUsage>(onEvent, path as string, () => ({
    provider: "iCloud", name: "iCloud Drive", path: path as string, canEvict: (path as string).includes("Mobile Documents"),
    files: 18_420, totalBytes: 212e9, localBytes: 64e9, cloudOnlyFiles: 9_870, cancelled: false,
    largestLocal: [
      fakeFile(`${HOME}/Library/Mobile Documents/com~apple~CloudDocs/Vídeos/Formatura.mov`, 12.4e9, 400, "video"),
      fakeFile(`${HOME}/Library/Mobile Documents/com~apple~CloudDocs/Backups/fotos-2023.zip`, 8.1e9, 600, "archive"),
      fakeFile(`${HOME}/Library/Mobile Documents/com~apple~CloudDocs/Projetos/render-final.mp4`, 3.3e9, 90, "video"),
    ],
  }), 900),
);
registerMock("evict_icloud", ({ paths }) => (paths as string[]).map((p) => ({ path: p, ok: true, error: null })));
registerMock("start_update_check", ({ online, onEvent }) =>
  streamJob<UpdatesResult>(onEvent, "/Applications", () => ({
    checkedOnline: !!online,
    apps: [
      { path: "/Applications/Slack.app", name: "Slack", bundleId: "com.tinyspeck.slackmacgap", installed: "4.51.191", source: "appStore", latest: online ? "4.52.162" : null, updateAvailable: !!online, url: "https://apps.apple.com/app/slack/id803453959" },
      { path: "/Applications/NordVPN.app", name: "NordVPN", bundleId: "com.nordvpn.macos", installed: "10.10.1", source: "sparkle", latest: online ? "10.12.0" : null, updateAvailable: !!online },
      { path: "/Applications/iTerm.app", name: "iTerm2", bundleId: "com.googlecode.iterm2", installed: "3.7.3", source: "sparkle", latest: online ? "3.7.3" : null, updateAvailable: false },
      { path: "/Applications/Telegram.app", name: "Telegram", bundleId: "ru.keepcoder.Telegram", installed: "12.10", source: "appStore", latest: online ? "12.10" : null, updateAvailable: false },
      { path: "/Applications/Figma.app", name: "Figma", bundleId: "com.figma.Desktop", installed: "126.1", source: "none", updateAvailable: false },
    ],
  }), 700),
);
registerMock("open_app_store_page", () => null);
registerMock("open_application", () => null);

registerMock("start_security_audit", ({ onEvent }) =>
  streamJob<SecurityAudit>(onEvent, "/Library", () => ({
    persistence: [
      { scope: "user", plist: `${HOME}/Library/LaunchAgents/com.apple.update.helper.plist`, label: "com.apple.update.helper", program: "/Users/Shared/.cache/helper", arguments: [], runAtLoad: true, keepAlive: true, disabled: false, programExists: true, signature: { kind: "unsigned" }, app: null, findings: [{ level: "attention", code: "unsigned" }, { level: "attention", code: "applePrefixNotApple" }, { level: "attention", code: "temporaryLocation" }] },
      { scope: "user", plist: `${HOME}/Library/LaunchAgents/com.glaido.app.plist`, label: "com.glaido.app", program: "/Applications/Glaido.app/Contents/MacOS/Glaido", arguments: [], runAtLoad: true, keepAlive: false, disabled: false, programExists: false, signature: null, app: null, findings: [{ level: "info", code: "programMissing" }] },
      { scope: "allUsers", plist: "/Library/LaunchAgents/us.zoom.updater.plist", label: "us.zoom.updater", program: "/Library/Application Support/zoom.us/ZoomUpdater", arguments: [], runAtLoad: true, keepAlive: false, disabled: false, programExists: true, signature: { kind: "developerId", teamId: "BJ4HAAB9B3" }, app: null, findings: [] },
    ],
    apps: [
      { path: "/Applications/MaxPlayer.app", name: "MaxPlayer", signature: { kind: "adHoc" }, gatekeeper: null, gatekeeperChecked: true, quarantined: true, findings: [{ level: "notice", code: "adhoc" }, { level: "notice", code: "gatekeeperRejected" }] },
      { path: "/Applications/Figma.app", name: "Figma", signature: { kind: "developerId", teamId: "T8RA8NE3B7" }, gatekeeper: null, gatekeeperChecked: false, quarantined: false, findings: [] },
    ],
  } as SecurityAudit), 900),
);

// ---- Developer Care (M12): fictitious repositories covering every state the screen has to show.
const devRoots: DevRoot[] = [{ path: `${HOME}/Projects`, addedAt: now - 20 * DAY }];
const devSnapshots: SnapshotRecord[] = [];
const removedArtifacts = new Set<string>();
const devVisibility: Record<string, { v: "public" | "privateOrMissing" | "unknown"; at: number }> = {};

const gh = (name: string, owner = "demo"): RemoteView => ({ name: "origin", url: `git@github.com:${owner}/${name}.git`, kind: { kind: "github", owner, repo: name } });
const st = (o: Partial<NonNullable<RepoReport["status"]>> = {}): NonNullable<RepoReport["status"]> => ({
  branch: "main", detached: false, noCommits: false, upstream: "origin/main", ahead: 0, behind: 0, changed: 0, untracked: 0, conflicted: 0, ...o,
});
const repo = (name: string, over: Partial<RepoReport>): RepoReport => ({
  path: `${HOME}/Projects/${name}`, root: `${HOME}/Projects`, gitFile: false, parent: null, status: st(), remotes: [gh(name)], submodules: [], embedded: [],
  error: null, artifactBytes: 0, lastSnapshot: null, ...over,
});
const art = (name: string, sub: string, kind: string, mb: number, over: Partial<DevArtifact> = {}): DevArtifact => ({
  path: `${HOME}/Projects/${name}/${sub}`, project: `${HOME}/Projects/${name}`, repo: `${HOME}/Projects/${name}`, kind, size: mb * 1e6, lastUsedMs: now - 12 * DAY,
  reason: kind === "node-modules" ? "Dependências instaladas. Voltam com npm, yarn ou pnpm install a partir do package.json." : kind === "rust-target" ? "Saída de compilação do Rust. Volta com cargo build." : "Saída de build. Volta com o script de build do projeto.",
  confidence: "safe", notes: [], cleanupRuleId: null, ...over,
});

function devReport(): DevReport {
  const artifacts: DevArtifact[] = [
    art("AudioPlayground", "node_modules", "node-modules", 145.6),
    art("AudioPlayground", "dist", "js-dist", 3),
    art("AudioPlayground", "ui/node_modules", "node-modules", 2.8, { confidence: "review", notes: ["noLockfile"], lastUsedMs: now - 200 * DAY }),
    art("Nodi", "src-tauri/target", "rust-target", 1222.6, { lastUsedMs: now - 3 * DAY }),
    art("Nodi", "node_modules", "node-modules", 206.7),
    art("Relay", "apps/mobile/ios/Pods", "cocoapods", 1238.6, { lastUsedMs: now - 95 * DAY }),
    art("Relay", "dist", "js-dist", 27.8, { confidence: "review", notes: ["notIgnored"] }),
    art("tattoo_tips", "node_modules", "node-modules", 323.2, { lastUsedMs: now - 400 * DAY }),
    art("legacy-site", "build", "build-output", 61, { confidence: "review", notes: ["tracked"] }),
  ];
  const live = artifacts.filter((a) => !removedArtifacts.has(a.path));
  const repos: RepoReport[] = [
    repo("AudioPlayground", { status: st({ ahead: 2, changed: 1 }), artifactBytes: 152.4e6 }),
    repo("Nodi", { status: st({ branch: "test/gpt-5.4-transcription", upstream: null, ahead: null, behind: null, changed: 33 }), artifactBytes: 1429.3e6 }),
    repo("Relay", { status: st({ changed: 20, untracked: 5 }), artifactBytes: 1266.4e6 }),
    repo("tattoo_tips", { status: st({ branch: "master", upstream: "origin/master", behind: 8, changed: 41, untracked: 3 }), artifactBytes: 323.2e6 }),
    repo("GymApp", { status: st(), submodules: ["Packages/GymCore"] }),
    repo("GymApp/Packages/GymCore", { path: `${HOME}/Projects/GymApp/Packages/GymCore`, parent: `${HOME}/Projects/GymApp`, gitFile: true, status: st({ upstream: null, ahead: null, behind: null }) }),
    repo("legacy-site", { status: st({ branch: "master", upstream: null, ahead: null, behind: null, changed: 4 }), remotes: [], embedded: ["vendor/old-lib"], artifactBytes: 61e6 }),
    repo("dotfiles", { remotes: [{ name: "backup", url: "/Volumes/Backup/dotfiles.git", kind: { kind: "local" } }], status: st({ upstream: "backup/main" }) }),
    repo("broken-repo", { status: null, remotes: [], error: "git status: fatal: detected dubious ownership in repository" }),
  ];
  for (const r of repos) r.lastSnapshot = devSnapshots.filter((x) => x.repo === r.path).sort((a, b) => b.createdAt - a.createdAt)[0] ?? null;
  for (const r of repos) r.artifactBytes = live.filter((a) => a.repo === r.path).reduce((n, a) => n + a.size, 0);
  return { roots: devRoots.map((r) => r.path), repos, artifacts: live, gitError: null, scannedAt: Date.now() };
}

const today = () => new Date().toISOString().slice(0, 10);
const repoName = (p: string) => p.split("/").pop() ?? p;
const hasPath = (list: string[], path: string) => list.some((e) => path === e || path.startsWith(`${e}/`));

function snapshotPlan(repoPath: string, excludes: string[]): SnapshotPlan {
  const name = repoName(repoPath);
  const found = devReport().repos.find((r) => r.path === repoPath);
  const s = found?.status;
  const secrets: SecretFinding[] =
    name === "Nodi"
      ? [
          { path: ".env", kind: "envFile", source: "name", tracked: false },
          { path: "keys/deploy.pem", kind: "privateKeyBlock", source: "content", tracked: false },
        ].filter((f) => !hasPath(excludes, f.path)) as SecretFinding[]
      : [];
  const untracked = [
    ...(name === "Nodi" && !hasPath(excludes, "capture.mov") ? [{ path: "capture.mov", size: 120e6 }] : []),
    { path: "notes/ideas.md", size: 4200 },
    { path: "scratch/test.ts", size: 1300 },
  ];
  const blockers: Blocker[] = [];
  if (secrets.length) blockers.push({ code: "secrets", count: secrets.length });
  untracked.filter((f) => f.size > 50e6).forEach((f) => blockers.push({ code: "fileTooBig", path: f.path, size: f.size }));
  if (name === "legacy-site") blockers.push({ code: "filtersConfigured" });
  if (name === "GymApp" || (s && s.changed + s.untracked === 0 && name !== "dotfiles")) blockers.push({ code: "nothingToSave" });
  return {
    repo: repoPath, branch: s?.branch ?? null, head: "e7ca3cd643065a1a23641b0d0a6b27d390e513df", refName: `wip/snapshot-${today()}`,
    changed: s?.changed ?? 0, untrackedCount: untracked.length, untrackedBytes: untracked.reduce((a, f) => a + f.size, 0), untracked,
    excludes, secrets, trackedSecrets: name === "AudioPlayground" ? 1 : 0, warnings: [], blockers,
  };
}

function pushPlan(repoPath: string, refName: string, remote: string): PushPlan {
  const r = devReport().repos.find((x) => x.path === repoPath);
  const rv = r?.remotes.find((x) => x.name === remote) ?? r?.remotes[0];
  if (!rv) return Promise.reject({ kind: "invalid", message: `remote desconhecido: ${remote}` }) as never;
  const cached = devVisibility[`${repoPath}|${rv.url}`];
  const kind = rv.kind;
  const warning = kind.kind === "local" ? null : cached?.v === "privateOrMissing" ? null : cached?.v === "public" ? "public" : "unknown";
  return {
    repo: repoPath, remote: rv.name, url: rv.url, kind, refName, commit: "e9c43389c57cb6a7ff4a80533067e28c35d77008",
    visibility: cached?.v ?? null, visibilityCheckedAt: cached?.at ?? null, warning, remotes: r?.remotes.map((x) => x.name) ?? [],
  };
}

registerMock("list_dev_roots", () => devRoots);
registerMock("add_dev_root", ({ path }) => {
  if (!devRoots.some((r) => r.path === path)) devRoots.push({ path: String(path), addedAt: Date.now() });
  return null;
});
registerMock("remove_dev_root", ({ path }) => {
  const i = devRoots.findIndex((r) => r.path === path);
  if (i >= 0) devRoots.splice(i, 1);
  return null;
});
registerMock("start_dev_scan", ({ onEvent }) => streamJob<DevReport>(onEvent, `${HOME}/Projects`, devReport, 1400));
registerMock("scan_repo_secrets", ({ repo: r }) => ({ findings: snapshotPlan(String(r), []).secrets, filesChecked: 412, contentSkipped: 31, unreadable: 0, truncated: false }));
registerMock("plan_wip_snapshot", ({ repo: r, excludes }) => snapshotPlan(String(r), (excludes as string[]) ?? []));
registerMock("create_wip_snapshot", ({ repo: r, excludes }) => {
  const plan = snapshotPlan(String(r), (excludes as string[]) ?? []);
  if (plan.blockers.length) return Promise.reject({ kind: "invalid", message: "o snapshot não pode ser criado: " + JSON.stringify(plan.blockers) });
  const rec: SnapshotRecord = { id: crypto.randomUUID(), repo: String(r), refName: plan.refName, commitId: "e9c43389c57cb6a7ff4a80533067e28c35d77008", createdAt: Date.now(), pushedAt: null };
  devSnapshots.push(rec);
  return { repo: String(r), refName: plan.refName, commit: rec.commitId, tree: "a".repeat(40), parent: plan.head, warnings: [] };
});
registerMock("remove_dev_artifacts", ({ items }) =>
  (items as { path: string; size: number }[]).map((i) => {
    const tracked = devReport().artifacts.find((a) => a.path === i.path)?.notes.includes("tracked");
    if (tracked) return { path: i.path, size: i.size, ok: false, error: "tem arquivos versionados no git: remova pelo git, não por aqui" };
    removedArtifacts.add(i.path);
    return { path: i.path, size: i.size, ok: true, error: null };
  }),
);
registerMock("list_dev_snapshots", () => devSnapshots);
registerMock("plan_push_snapshot", ({ repo: r, refName, remote }) => pushPlan(String(r), String(refName), String(remote)));
registerMock("check_remote_visibility", ({ repo: r, refName, remote }) => {
  const plan = pushPlan(String(r), String(refName), String(remote));
  if (plan.kind.kind !== "github") return Promise.reject({ kind: "invalid", message: "só é possível verificar remotes do github.com" });
  devVisibility[`${r}|${plan.url}`] = { v: repoName(String(r)) === "Nodi" ? "privateOrMissing" : "public", at: Date.now() };
  return pushPlan(String(r), String(refName), String(remote));
});
registerMock("push_snapshot", ({ repo: r, refName, remote, confirmed, acceptExposure }) => {
  const plan = pushPlan(String(r), String(refName), String(remote));
  if (!confirmed) return Promise.reject({ kind: "invalid", message: "o envio precisa ser confirmado" });
  if (plan.warning && !acceptExposure) return Promise.reject({ kind: "invalid", message: "este remote é público ou de visibilidade desconhecida: aceite o aviso para enviar" });
  const snap = devSnapshots.find((x) => x.repo === r && x.refName === refName);
  if (snap) snap.pushedAt = Date.now();
  return { remote: plan.remote, url: plan.url, refName: plan.refName, commit: plan.commit };
});

// ---- Backup Guard (M13): four scenarios, switchable from the console with __setBackupScenario("strong").
type BackupScenario = "ok" | "warn" | "strong" | "unknown";
let backupScenario: BackupScenario = "warn";
(window as unknown as { __setBackupScenario: (s: BackupScenario) => void }).__setBackupScenario = (s) => {
  backupScenario = s;
};
let backupStaleDays = 7;

function backupState(): BackupState {
  const sc = backupScenario;
  const last = sc === "strong" ? now - 30 * DAY : now - 2 * 3_600_000;
  const known = sc !== "unknown";
  return {
    timeMachine: {
      status: sc === "unknown" ? "unknown" : sc === "strong" ? "stale" : "recent",
      destinations: [{ id: "D1", name: "TM-MacBook", kind: "Local", mounted: sc === "ok", quotaGb: 600, bytesUsed: 181e9, bytesAvailable: 419e9, encrypted: true, lastBackupMs: known ? last : null, lastAttemptMs: known ? last : null }],
      autoBackup: sc === "ok" ? true : sc === "unknown" ? null : false,
      lastBackupMs: known ? last : null,
      lastBackupSource: known ? "preferences" : null,
      lastAttemptMs: known ? last : null,
      ageDays: known ? Math.floor((now - last) / DAY) : null,
      running: false,
      percent: null,
      localSnapshotCount: 2,
      newestLocalSnapshotMs: now - 3_600_000,
      unreadable: sc === "unknown" ? [{ source: "preferences", reason: "permissionDenied", detail: "Operation not permitted" }, { source: "destinationInfo", reason: "unavailable", detail: null }] : [],
    },
    icloud: { driveFolderPresent: true, signedIn: true, driveEnabled: true, desktopSynced: true, documentsSynced: true, inferred: true, unreadable: [] },
    fullDiskAccess: "denied",
    staleDays: backupStaleDays,
    readAt: Date.now(),
  };
}

function backupGate(): BackupGate {
  const sc = backupScenario;
  const unpushed = sc === "ok" ? [] : [
    { path: `${HOME}/Projects/AudioPlayground`, reasons: ["uncommitted", "unpushedCommits"] },
    { path: `${HOME}/Projects/Nodi`, reasons: ["uncommitted", "noUpstream"] },
    { path: `${HOME}/Projects/legacy-site`, reasons: ["noRemote"] },
  ];
  const level = sc === "ok" ? "ok" : sc === "warn" ? "warn" : "strong";
  const reasons = sc === "ok" ? [] : sc === "warn" ? ["unpushedRepos", "autoBackupOff"] : sc === "strong" ? ["stale", "unpushedRepos", "autoBackupOff"] : ["unknown", "unpushedRepos"];
  return { level, reasons, unpushed, unreadableRepos: 0, backup: backupState() };
}

/** Mirrors the Rust check: the answer from the UI is verified against the gate. */
function gateError(ack: BackupAck | undefined) {
  const level = backupGate().level;
  if (level === "ok") return null;
  if (level === "warn" && ack?.acknowledged) return null;
  if (level === "strong" && ack?.acknowledged && (ack.typed ?? "").trim().toUpperCase() === "CONFIRMO") return null;
  return { kind: "backupGate", message: level === "warn" ? "há um aviso sobre o backup: confirme que viu antes de continuar" : "não há backup recente: digite CONFIRMO para continuar" };
}

registerMock("get_backup_state", () => backupState());
registerMock("get_backup_gate", () => backupGate());
registerMock("set_backup_stale_days", ({ days }) => {
  backupStaleDays = Number(days);
  return null;
});
for (const name of ["run_smart_care", "run_cleanup_rules", "remove_orphans", "move_duplicates_to_trash", "remove_dev_artifacts"]) {
  const inner = handlers[name];
  handlers[name] = (args) => {
    const err = gateError(args.backupAck as BackupAck | undefined);
    return err ? Promise.reject(err) : inner(args);
  };
}

// ---- Time Machine exclusions (M13)
const tmExcluded = new Map<string, TmExclusion>([
  [`${HOME}/Projects/Relay/apps/desktop/node_modules`, { path: `${HOME}/Projects/Relay/apps/desktop/node_modules`, addedAt: now - 10 * DAY, kind: "node-modules", state: "lost" }],
  [`${HOME}/Projects/old-app/target`, { path: `${HOME}/Projects/old-app/target`, addedAt: now - 40 * DAY, kind: "rust-target", state: "missing" }],
]);
registerMock("preview_tm_exclusions", ({ items }): ExclusionPreview => {
  const list = (items as { path: string; repo: string; kind: string; size: number }[]).map((i) => {
    const tracked = devReport().artifacts.find((a) => a.path === i.path)?.notes.includes("tracked");
    const already = tmExcluded.get(i.path)?.state === "active";
    const ok = !tracked && !already;
    return { path: i.path, repo: i.repo, kind: i.kind, size: i.size, state: already ? "excluded" : "included", ok, reason: tracked ? "tem arquivos versionados no git: remova pelo git, não por aqui" : already ? "já está fora do backup" : null } as ExclusionPreview["items"][number];
  });
  const okItems = list.filter((i) => i.ok);
  return { items: list, count: okItems.length, bytes: okItems.reduce((n, i) => n + i.size, 0) };
});
registerMock("apply_tm_exclusions", ({ items }) =>
  (items as { path: string; size: number; kind: string }[]).map((i) => {
    tmExcluded.set(i.path, { path: i.path, addedAt: Date.now(), kind: i.kind, state: "active" });
    return { path: i.path, size: i.size, ok: true, error: null };
  }),
);
registerMock("list_tm_exclusions", () => [...tmExcluded.values()].sort((a, b) => a.path.localeCompare(b.path)));
registerMock("undo_tm_exclusions", ({ paths }) =>
  (paths as string[]).map((path) => {
    const had = tmExcluded.delete(path);
    return { path, ok: had, gone: false, stillExcluded: false, error: had ? null : "esta exclusão não foi feita pelo OrganizeMyMac" };
  }),
);

// ---- Quarantine (M14)
let removalMode: RemovalMode = "trash";
const quarantined: QuarantineView[] = [
  { id: "a1b2c3d4", origin: `${HOME}/Downloads/Figma-126.1.dmg`, dest: `${HOME}/Library/Application Support/dev.galuppo.OrganizeMyMac/Quarantine/2026-10-12/a1b2c3d4-Figma-126.1.dmg`, size: 540e6, quarantinedAt: now - DAY, present: true, logPath: `${HOME}/Library/Application Support/dev.galuppo.OrganizeMyMac/Quarantine/2026-10-12/quarantine.csv` },
  { id: "e5f6a7b8", origin: `${HOME}/Projects/Relay/apps/mobile/ios/Pods`, dest: `${HOME}/Library/Application Support/dev.galuppo.OrganizeMyMac/Quarantine/2026-10-12/e5f6a7b8-Pods`, size: 1.2e9, quarantinedAt: now - DAY, present: true, logPath: `${HOME}/Library/Application Support/dev.galuppo.OrganizeMyMac/Quarantine/2026-10-12/quarantine.csv` },
  { id: "c9d0e1f2", origin: `${HOME}/Documents/Old/contrato-2019.pdf`, dest: `${HOME}/Library/Application Support/dev.galuppo.OrganizeMyMac/Quarantine/2026-10-05/c9d0e1f2-contrato-2019.pdf`, size: 4.1e6, quarantinedAt: now - 8 * DAY, present: false, logPath: `${HOME}/Library/Application Support/dev.galuppo.OrganizeMyMac/Quarantine/2026-10-05/quarantine.csv` },
];
registerMock("get_removal_mode", () => removalMode);
registerMock("set_removal_mode", ({ mode }) => {
  removalMode = mode as RemovalMode;
  return null;
});
registerMock("list_quarantine", (): QuarantineList => ({ items: [...quarantined], totalBytes: quarantined.filter((q) => q.present).reduce((n, q) => n + q.size, 0) }));
registerMock("restore_quarantine", ({ ids, to, confirmed }) => {
  if (!confirmed) return Promise.reject({ kind: "invalid", message: "restaurar precisa ser confirmado" });
  return (ids as string[]).map((id) => {
    const i = quarantined.findIndex((q) => q.id === id);
    const q = quarantined[i];
    if (!q) return { path: id, size: 0, ok: false, error: "este item não está na quarentena" };
    // One item always finds its place taken, so the "other folder" path can be tried.
    if (!to && q.origin.endsWith("Figma-126.1.dmg")) return { path: q.origin, size: q.size, ok: false, error: `já existe algo em ${q.origin}: escolha outra pasta` };
    quarantined.splice(i, 1);
    return { path: to ? `${to}/${q.origin.split("/").pop()}` : q.origin, size: q.size, ok: true, error: null };
  });
});
registerMock("empty_quarantine", ({ ids, confirmed }) => {
  if (!confirmed) return Promise.reject({ kind: "invalid", message: "esvaziar a quarentena precisa ser confirmado" });
  return (ids as string[]).map((id) => {
    const i = quarantined.findIndex((q) => q.id === id);
    const q = quarantined[i];
    if (!q) return { path: id, size: 0, ok: false, error: "este item não está na quarentena" };
    quarantined.splice(i, 1);
    return { path: q.dest, size: q.size, ok: true, error: null };
  });
});

// ---- Report (M14)
const mockSave = { exists: false };
(window as unknown as { __setSaveExists: (v: boolean) => void }).__setSaveExists = (v) => {
  mockSave.exists = v;
};
registerMock("plugin:dialog|save", ({ options }) => {
  const filters = (options as { filters?: { extensions: string[] }[] } | undefined)?.filters;
  const ext = filters?.[0]?.extensions?.[0] ?? "md";
  return `${HOME}/Desktop/${mockSave.exists ? "ja-existe" : "relatorio-organizemymac"}.${ext}`;
});
registerMock("generate_report", ({ periodDays }): Report => {
  const g = backupGate();
  return {
    generatedAt: Date.now(),
    periodDays: Number(periodDays),
    groups: [
      { action: "trash", label: "Movido para a Lixeira", total: 14, ok: 13, bytes: 1.4e9 },
      { action: "quarantine", label: "Movido para a Quarentena", total: 3, ok: 3, bytes: 1.74e9 },
      { action: "tmExclude", label: "Excluído do backup do Time Machine", total: 6, ok: 6, bytes: 0 },
      { action: "snapshot", label: "Snapshot do trabalho criado", total: 2, ok: 2, bytes: 0 },
    ],
    recent: [
      { whenMs: now - 3_600_000, action: "quarantine", label: "Movido para a Quarentena", path: `${HOME}/Downloads/Figma-126.1.dmg`, size: 540e6, ok: true, detail: null },
      { whenMs: now - 7_200_000, action: "trash", label: "Movido para a Lixeira", path: `${HOME}/Projects/Relay/node_modules`, size: 732e6, ok: true, detail: null },
      { whenMs: now - 2 * DAY, action: "trash", label: "Movido para a Lixeira", path: `${HOME}/Projects/legacy-site/build`, size: 61e6, ok: false, detail: "tem arquivos versionados no git: remova pelo git, não por aqui" },
      ...Array.from({ length: 9 }, (_, i) => ({ whenMs: now - (3 + i) * DAY, action: "trash", label: "Movido para a Lixeira", path: `${HOME}/Library/Caches/app-${i}`, size: 20e6, ok: true, detail: null })),
    ],
    recentOmitted: 0,
    snapshots: [{ whenMs: now - DAY, repo: `${HOME}/Projects/Nodi`, refName: "wip/snapshot-2026-10-12", pushed: false }],
    backup: { status: g.backup.timeMachine.status, lastBackupMs: g.backup.timeMachine.lastBackupMs, autoBackup: g.backup.timeMachine.autoBackup, staleDays: 7 },
    pending: [
      { code: "autoBackupOff", title: "Backups automáticos desligados", detail: "Um novo backup só acontece se alguém o disparar.", count: 0, bytes: 0, items: [] },
      { code: "unpushedRepos", title: "3 repositórios com trabalho só neste Mac", detail: "Alterações sem commit, commits não enviados ou branches sem destino.", count: 3, bytes: 0, items: g.unpushed.map((u) => ({ text: u.path, note: u.reasons.map((r) => ({ uncommitted: "alterações sem commit", unpushedCommits: "commits não enviados", noUpstream: "branch sem upstream", noRemote: "sem remote" } as Record<string, string>)[r] ?? r).join(", ") })) },
      { code: "quarantine", title: "3 itens na Quarentena", detail: "Continuam ocupando espaço: restaurar ou enviar à Lixeira.", count: 3, bytes: 1.7e9, items: [{ text: `${HOME}/Documents/Old/contrato-2019.pdf`, note: "não está mais na pasta" }] },
    ],
    next: { code: "snapshot", text: "Faça um snapshot de 3 repositórios: o que está lá só existe neste Mac." },
  };
});
registerMock("export_report", ({ path, overwrite }) => {
  if (String(path).includes("ja-existe") && !overwrite) return Promise.reject({ kind: "exists", message: `já existe um arquivo em ${path}` });
  return { path, bytes: 4096 };
});

// ---- Homebrew (M14)
registerMock("analyze_homebrew", (): HomebrewReport => {
  const pkg = (name: string, kind: "formula" | "cask", installed: string, current: string, change: HomebrewReport["packages"][number]["change"], risk: HomebrewReport["packages"][number]["risk"], runtime = false, projects: HomebrewReport["packages"][number]["projects"] = [], pinned = false) => ({
    name, kind, installed: [installed], current, pinned, change, risk, runtime, upgradeCommand: `brew upgrade${kind === "cask" ? " --cask" : ""} ${name}`, projects,
  });
  return {
    installed: true,
    error: null,
    reposScanned: 9,
    packages: [
      pkg("node", "formula", "20.11.1", "22.1.0", "major", "high", true, [
        { repo: `${HOME}/Projects/Relay`, via: "Brewfile", exact: true },
        { repo: `${HOME}/Projects/AudioPlayground`, via: "package.json", exact: false },
        { repo: `${HOME}/Projects/Nodi`, via: "package.json", exact: false },
        { repo: `${HOME}/Projects/tattoo_tips`, via: ".nvmrc", exact: false },
      ]),
      pkg("postgresql@14", "formula", "14.10", "14.11", "minor", "high", true),
      pkg("python@3.11", "formula", "3.11.7", "3.12.2", "minor", "high", true, [{ repo: `${HOME}/Projects/legacy-site`, via: "requirements.txt", exact: false }]),
      pkg("git", "formula", "2.43.0", "2.44.0", "minor", "medium"),
      pkg("figma", "cask", "124.0", "126.1", "major", "medium"),
      pkg("wget", "formula", "1.21.4", "1.21.4_1", "patch", "low"),
      pkg("docker", "cask", "4.27.0", "4.28.0", "minor", "low", false, [{ repo: `${HOME}/Projects/Relay`, via: "Brewfile", exact: true }]),
      pkg("ffmpeg", "formula", "6.1", "7.0", "major", "pinned", false, [], true),
    ],
  };
});

export function install() {
  mockIPC((cmd, args) => {
    const handler = handlers[cmd];
    if (!handler) {
      console.warn("[mock] no handler for", cmd, args);
      return null;
    }
    return handler((args ?? {}) as Record<string, unknown>);
  });
  document.documentElement.dataset.mock = "true";
}
