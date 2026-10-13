import { useState } from "react";
import { Sidebar } from "./Sidebar";
import { useNav } from "../stores/nav";
import { moduleOf } from "./modules";
import { ScannerPage } from "../features/scanner/ScannerPage";
import { SettingsPage } from "../features/settings/SettingsPage";
import { OverviewPage } from "../features/dashboard/OverviewPage";
import { LargeFilesPage } from "../features/storage/LargeFilesPage";
import { DownloadsPage } from "../features/storage/DownloadsPage";
import { TrashPage } from "../features/storage/TrashPage";
import { DuplicatesPage } from "../features/duplicates/DuplicatesPage";
import { SpaceMapPage } from "../features/spacemap/SpaceMapPage";
import { AppsPage } from "../features/applications/AppsPage";
import { LeftoversPage } from "../features/applications/LeftoversPage";
import { PerformancePage } from "../features/performance/PerformancePage";
import { SimilarImagesPage } from "../features/images/SimilarImagesPage";
import { SmartCarePage } from "../features/smartcare/SmartCarePage";
import { AdvancedCleanupPage } from "../features/cleanup/AdvancedCleanupPage";
import { CloudPage } from "../features/cloud/CloudPage";
import { UpdatesPage } from "../features/applications/UpdatesPage";
import { SecurityPage } from "../features/security/SecurityPage";
import { ProjectsPage } from "../features/dev/ProjectsPage";
import { BackupPage } from "../features/backup/BackupPage";
import { BackupGateHost } from "../features/backup/BackupGateDialog";
import { Onboarding, hasOnboarded } from "../features/onboarding/Onboarding";

export function App() {
  const section = useNav((s) => s.section);
  const [onboarded, setOnboarded] = useState(hasOnboarded);

  return (
    <div data-tint={moduleOf(section)?.id ?? "default"} className="page-canvas flex h-full">
      <Sidebar />
      <main className="min-w-0 flex-1 overflow-hidden">
        {section === "overview" && <OverviewPage />}
        {section === "smartCare" && <SmartCarePage />}
        {section === "scanner" && <ScannerPage />}
        {section === "spaceMap" && <SpaceMapPage />}
        {section === "largeFiles" && <LargeFilesPage />}
        {section === "downloads" && <DownloadsPage />}
        {section === "duplicates" && <DuplicatesPage />}
        {section === "similarImages" && <SimilarImagesPage />}
        {section === "cleanup" && <AdvancedCleanupPage />}
        {section === "cloud" && <CloudPage />}
        {section === "updates" && <UpdatesPage />}
        {section === "trash" && <TrashPage />}
        {section === "apps" && <AppsPage />}
        {section === "leftovers" && <LeftoversPage />}
        {section === "performance" && <PerformancePage />}
        {section === "security" && <SecurityPage />}
        {section === "projects" && <ProjectsPage />}
        {section === "backup" && <BackupPage />}
        {section === "settings" && <SettingsPage />}
      </main>
      <BackupGateHost />
      {!onboarded && <Onboarding onDone={() => setOnboarded(true)} />}
    </div>
  );
}
