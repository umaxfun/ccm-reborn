import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import "./styles.css";
import type {
  Catalog, DryRunPlan, GameDirectoryCandidate, Inspection,
  InstallProgress, RestoreResult, SavedCampaignResume, StarcraftProfileCandidate,
} from "./types";
import { renderPlanDialog } from "./render-plan";
import { renderLibrary } from "./render-library";
import { isCurrentCatalogCampaign } from "./domain";
import { profileName, resumeFor } from "./resume";
import { migrateLegacyProfile, renderLegacyMigration } from "./legacy-migration";
import { renderDashboard } from "./render-dashboard";
import { copyInstallDiagnostics, InstallFailure, openInstallLogFolder, renderInstallStatus } from "./install-status";
import { applyCampaignInstall, planCampaignInstall } from "./install-flow";

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) throw new Error("Application root is missing.");
const root = app;

const communityCatalog = "https://files.ccm-reborn.mikilabs.io/catalog.json";
const localDevCatalog = "../dev-catalog/catalog.json";
const defaultCatalog = import.meta.env.DEV ? localDevCatalog : communityCatalog;
let catalogSource = localStorage.getItem("ccm-catalog-source")?.trim() || defaultCatalog;
let gameDir = localStorage.getItem("ccm-game-directory") ?? "";
let profileDir = localStorage.getItem("ccm-profile-directory") ?? "";
let profileCandidates: StarcraftProfileCandidate[] = [];
let catalog: Catalog | null = null;
let inspection: Inspection | null = null;
let savedResumes: SavedCampaignResume[] = [];
let page: "dashboard" | "library" = "dashboard";
let selectedId = "";
let message = "Getting your campaigns ready…";
let messageKind: "neutral" | "success" | "error" = "neutral";
let busy = false;
let battleNetMessageTimer: number | undefined;
let pendingPlan: DryRunPlan | null = null;
let installActivity: InstallProgress | null = null;
let activeInstallCampaignId = "";
let diagnosticLogPath = "";
let lastInstallFailure: InstallFailure | null = null;
let scrollToInstallStatus = false;
let settingsOpen = false;
let settingsCatalogSourceDraft = catalogSource;
let settingsProfileDirDraft = profileDir;

function profileOptionLabel(path: string, index: number) {
  const parts = path.split(/[\\/]/).filter(Boolean);
  const profile = profileName(path);
  const account = parts.at(-2);
  return `${account ? `${account} / ` : ""}${profile}${index === 0 ? " (recent)" : ""}`;
}

const escapeHtml = (value: string) => value.replace(/[&<>'"]/g, (character) =>
  ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character]!,
);

const installFlow = {
  catalog: () => catalog, gameDir: () => gameDir, profileDir: () => profileDir,
  pendingPlan: () => pendingPlan, activity: () => installActivity,
  setBusy: (value: boolean) => { busy = value; },
  setPendingPlan: (value: DryRunPlan | null) => { pendingPlan = value; },
  setActivity: (value: InstallProgress | null) => { installActivity = value; },
  setCampaignId: (value: string) => { activeInstallCampaignId = value; },
  setFailure: (value: InstallFailure | null) => { lastInstallFailure = value; },
  setMessage: (value: string, kind: "neutral" | "success" | "error") => { message = value; messageKind = kind; },
  inspectDirectory, render,
};

function render() {
  root.innerHTML = `
    <aside class="sidebar">
      <div class="brand"><span class="brand-mark">C</span><span>CCM <b>REBORN</b></span></div>
      <p class="eyebrow">CAMPAIGN CONTROL</p>
      <nav>
        <button class="nav-item ${page === "dashboard" ? "active" : ""}" data-page="dashboard"><span>◈</span> Campaigns <strong>4</strong></button>
        <button class="nav-item ${page === "library" ? "active" : ""}" data-page="library"><span>▦</span> Library <strong>${catalog?.campaigns.length ?? 0}</strong></button>
        <button class="nav-item" data-action="show-settings"><span>⚙</span> Sources</button>
      </nav>
      <div class="sidebar-footer">
        <div class="dot ${gameDir ? "" : "muted"}"></div>
        <div><small>STARCRAFT II</small><br />${gameDir ? "Detected" : "Not found"}</div>
      </div>
    </aside>
    <main>
      <header>
        <div><h1>${page === "dashboard" ? "Campaigns" : "Library"}</h1></div>
        <div class="header-actions">
          <button class="ghost compact" data-action="refresh-catalog" ${busy || !catalogSource ? "disabled" : ""}>Check for updates</button><button class="settings-button" data-action="show-settings">Sources</button>
        </div>
      </header>
      ${renderInstallStatus({ message, messageKind, activity: installActivity, failure: lastInstallFailure, logPath: diagnosticLogPath })}
      ${renderLegacyMigration(catalog, savedResumes, profileDir, busy)}
      ${page === "dashboard" ? renderDashboard({ catalog, inspection, savedResumes, busy, gameDir, profileDir, installingCampaignId: activeInstallCampaignId, isCurrentCatalogCampaign: (campaign) => isCurrentCatalogCampaign(campaign, inspection) }) : renderLibrary({ catalog, inspection, selectedId, busy, gameDir, installingCampaignId: activeInstallCampaignId, isCurrentCatalogCampaign: (campaign) => isCurrentCatalogCampaign(campaign, inspection) })}
    </main>
    <dialog id="settings-dialog">
      <form method="dialog" class="dialog-card">
        <button class="close" value="cancel" aria-label="Close">×</button>
        <p class="eyebrow">SOURCES</p><h2>Catalog & game location</h2>
        <label>Catalog source
          <input id="catalog-source" spellcheck="false" value="${escapeHtml(settingsCatalogSourceDraft)}" placeholder="/path/to/catalog.json or https://…/catalog.json" />
          <small>Releases use the community cloud catalog by default. You can choose another HTTPS catalog or a local development catalog here.</small>
        </label>
        <div class="directory-actions"><button type="button" class="ghost" data-action="use-cloud-catalog">Use community cloud</button>${import.meta.env.DEV ? '<button type="button" class="ghost" data-action="use-local-catalog">Use local dev catalog</button>' : ""}</div>
        <section class="detected-directory">
          <small>STARCRAFT II DIRECTORY</small>
          <strong>${gameDir ? escapeHtml(gameDir) : "No installation detected"}</strong>
          <span>Auto-detection runs when CCM Reborn starts. Choose a folder only if it missed your installation.</span>
        </section>
        <div class="directory-actions">
          <button type="button" class="ghost" data-action="choose-directory">Choose folder…</button>
          <button type="button" class="ghost" data-action="detect-directory">Detect again</button>
        </div>
        <label>StarCraft II profile
          <select id="profile-directory">
            <option value="">Choose before applying an install…</option>
            ${profileCandidates.map((profile, index) => `<option value="${escapeHtml(profile.path)}" ${profile.path === settingsProfileDirDraft ? "selected" : ""}>${escapeHtml(profileOptionLabel(profile.path, index))}</option>`).join("")}
          </select>
          <small>This is the single account profile whose bank, saves, and campaign progress CCM may switch. The first detected profile is selected automatically; choose the other one only if that is the account you play on.</small>
          ${settingsProfileDirDraft ? `<small class="profile-path">${escapeHtml(settingsProfileDirDraft)}</small>` : ""}
        </label>
        <div class="directory-actions">
          <button type="button" class="ghost" data-action="choose-profile">Choose profile…</button>
          <button type="button" class="ghost" data-action="detect-profiles">Find profiles</button>
        </div>
        <div class="dialog-actions">
          <button class="ghost" value="cancel">Cancel</button>
          <button class="primary" value="default" data-action="save-settings">Reload catalog</button>
        </div>
      </form>
    </dialog>
    ${pendingPlan ? renderPlanDialog(pendingPlan, profileDir, busy) : ""}
  `;
  bindEvents();
  const settingsDialog = document.querySelector<HTMLDialogElement>("#settings-dialog");
  if (settingsOpen && settingsDialog && !settingsDialog.open) settingsDialog.showModal();
  const planDialog = document.querySelector<HTMLDialogElement>("#plan-dialog");
  if (planDialog && !planDialog.open) planDialog.showModal();
  if (scrollToInstallStatus) {
    scrollToInstallStatus = false;
    window.requestAnimationFrame(() => {
      document.querySelector<HTMLElement>(".status.installing")?.scrollIntoView({
        block: "start",
        behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth",
      });
    });
  }
}

function bindEvents() {
  document.querySelectorAll<HTMLButtonElement>("[data-page]").forEach((button) => {
    button.addEventListener("click", () => {
      page = button.dataset.page === "library" ? "library" : "dashboard";
      render();
    });
  });
  document.querySelectorAll<HTMLButtonElement>("[data-library-campaign]").forEach((button) => {
    button.addEventListener("click", () => {
      selectedId = button.dataset.libraryCampaign ?? "";
      render();
    });
  });
  document.querySelectorAll<HTMLButtonElement>("[data-action='show-settings']").forEach((button) => {
    button.addEventListener("click", () => showSettings());
  });
  document.querySelector<HTMLDialogElement>("#settings-dialog")?.addEventListener("close", () => {
    settingsOpen = false;
  });
  const catalogSourceInput = document.querySelector<HTMLInputElement>("#catalog-source");
  catalogSourceInput?.addEventListener("input", () => {
    settingsCatalogSourceDraft = catalogSourceInput.value;
  });
  const profileDirectorySelect = document.querySelector<HTMLSelectElement>("#profile-directory");
  profileDirectorySelect?.addEventListener("change", () => {
    settingsProfileDirDraft = profileDirectorySelect.value;
  });
  document.querySelector<HTMLButtonElement>("[data-action='save-settings']")?.addEventListener("click", (event) => {
    event.preventDefault();
    catalogSource = settingsCatalogSourceDraft.trim() || defaultCatalog;
    profileDir = settingsProfileDirDraft.trim();
    localStorage.setItem("ccm-catalog-source", catalogSource);
    if (profileDir) localStorage.setItem("ccm-profile-directory", profileDir);
    else localStorage.removeItem("ccm-profile-directory");
    settingsOpen = false;
    document.querySelector<HTMLDialogElement>("#settings-dialog")?.close();
    void loadCatalog();
  });
  document.querySelector<HTMLButtonElement>("[data-action='choose-directory']")?.addEventListener("click", () => void chooseGameDirectory());
  document.querySelector<HTMLButtonElement>("[data-action='refresh-catalog']")?.addEventListener("click", () => void loadCatalog());
  document.querySelector<HTMLButtonElement>("[data-action='use-cloud-catalog']")?.addEventListener("click", () => useCatalogSource(communityCatalog));
  document.querySelector<HTMLButtonElement>("[data-action='use-local-catalog']")?.addEventListener("click", () => useCatalogSource(localDevCatalog));
  document.querySelector<HTMLButtonElement>("[data-action='detect-directory']")?.addEventListener("click", () => void detectGameDirectory());
  document.querySelector<HTMLButtonElement>("[data-action='choose-profile']")?.addEventListener("click", () => void chooseProfileDirectory());
  document.querySelector<HTMLButtonElement>("[data-action='detect-profiles']")?.addEventListener("click", () => void refreshProfiles(true));
  document.querySelectorAll<HTMLButtonElement>("[data-action='install']").forEach((button) => {
    button.addEventListener("click", () => {
      scrollToInstallStatus = true;
      void planCampaignInstall(installFlow, button.dataset.campaign ?? "");
    });
  });
  document.querySelectorAll<HTMLButtonElement>("[data-action='close-plan']").forEach((button) => {
    button.addEventListener("click", () => {
      pendingPlan = null;
      installActivity = null;
      activeInstallCampaignId = "";
      render();
    });
  });
  document.querySelector<HTMLButtonElement>("[data-action='apply-plan']")?.addEventListener("click", () => {
    scrollToInstallStatus = true;
    void applyCampaignInstall(installFlow);
  });
  document.querySelectorAll<HTMLButtonElement>("[data-action='open-battle-net']").forEach((button) => button.addEventListener("click", () => void openBattleNet()));
  document.querySelectorAll<HTMLButtonElement>("[data-action='restore']").forEach((button) => {
    button.addEventListener("click", () => void restoreOriginals(button.dataset.target ?? ""));
  });
  document.querySelectorAll<HTMLButtonElement>("[data-action='migrate-legacy']").forEach((button) => {
    button.addEventListener("click", () => void migrateLegacySnapshot(button.dataset.campaign ?? ""));
  });
  document.querySelector<HTMLButtonElement>("[data-action='copy-diagnostics']")?.addEventListener("click", () => void copyDiagnostics());
  document.querySelector<HTMLButtonElement>("[data-action='open-log-folder']")?.addEventListener("click", () => void openDiagnosticLogFolder());
}

function showSettings() {
  settingsCatalogSourceDraft = catalogSource;
  settingsProfileDirDraft = profileDir;
  settingsOpen = true;
  const dialog = document.querySelector<HTMLDialogElement>("#settings-dialog");
  if (dialog && !dialog.open) dialog.showModal();
}

async function copyDiagnostics() {
  if (!lastInstallFailure) return;
  try {
    await copyInstallDiagnostics(lastInstallFailure, diagnosticLogPath);
    message = "Diagnostics copied to the clipboard.";
    messageKind = "success";
  } catch {
    message = "Could not copy diagnostics. The log path is shown below.";
    messageKind = "error";
  }
  render();
}

async function openDiagnosticLogFolder() {
  try {
    await openInstallLogFolder();
  } catch (error) {
    message = String(error);
    messageKind = "error";
    render();
  }
}

function useCatalogSource(source: string) {
  catalogSource = source;
  settingsCatalogSourceDraft = source;
  settingsOpen = false;
  localStorage.setItem("ccm-catalog-source", catalogSource);
  document.querySelector<HTMLDialogElement>("#settings-dialog")?.close();
  void loadCatalog();
}

async function resolveGameDirectory(path: string) {
  return invoke<GameDirectoryCandidate>("resolve_game_directory", { path });
}

async function chooseGameDirectory() {
  const selected = await open({ directory: true, multiple: false, title: "Choose StarCraft II or its installation folder" });
  if (typeof selected !== "string") return;
  try {
    const game = await resolveGameDirectory(selected);
    setGameDirectory(game);
    await refreshProfiles();
    message = `StarCraft II directory selected: ${game.label}`;
    messageKind = "success";
    await inspectDirectory();
  } catch (error) {
    message = String(error);
    messageKind = "error";
  }
  render();
}

async function chooseProfileDirectory() {
  const selected = await open({ directory: true, multiple: false, title: "Choose the StarCraft II account profile" });
  if (typeof selected !== "string") return;
  profileDir = selected;
  settingsProfileDirDraft = profileDir;
  localStorage.setItem("ccm-profile-directory", profileDir);
  if (!profileCandidates.some((profile) => profile.path === profileDir)) {
    profileCandidates = [...profileCandidates, { path: profileDir, label: profileDir }];
  }
  message = "StarCraft II account selected.";
  messageKind = "success";
  render();
}

async function refreshProfiles(showMessage = false) {
  try {
    profileCandidates = await invoke<StarcraftProfileCandidate[]>("detect_starcraft_profiles");
    // Detection is already ranked by the backend (active/recent profile
    // first). Select that concrete path automatically so the safety gate is
    // satisfied without making a first-run user open Settings. They can still
    // change it there before applying anything.
    if (!profileDir && profileCandidates.length) {
      profileDir = profileCandidates[0].path;
      if (!settingsProfileDirDraft) settingsProfileDirDraft = profileDir;
      localStorage.setItem("ccm-profile-directory", profileDir);
    }
    if (profileDir && !profileCandidates.some((profile) => profile.path === profileDir)) {
      profileCandidates = [...profileCandidates, { path: profileDir, label: profileDir }];
    }
    if (showMessage) {
      message = profileCandidates.length ? `${profileCandidates.length} StarCraft II profile(s) found.` : "No StarCraft II profiles were found.";
      messageKind = profileCandidates.length ? "success" : "error";
      render();
    }
  } catch (error) {
    if (showMessage) {
      message = String(error);
      messageKind = "error";
      render();
    }
  }
}

function setGameDirectory(game: GameDirectoryCandidate) {
  if (gameDir !== game.path) {
    profileDir = "";
    settingsProfileDirDraft = "";
    localStorage.removeItem("ccm-profile-directory");
  }
  gameDir = game.path;
  localStorage.setItem("ccm-game-directory", gameDir);
}

async function detectGameDirectory() {
  busy = true;
  message = "Looking for StarCraft II…";
  messageKind = "neutral";
  render();
  try {
    const locations = await invoke<GameDirectoryCandidate[]>("detect_game_directories");
    if (!locations.length) throw new Error("StarCraft II was not found in the standard locations. Choose its folder manually.");
    setGameDirectory(locations[0]);
    await refreshProfiles();
    message = `Found StarCraft II: ${locations[0].label}`;
    messageKind = "success";
    await inspectDirectory();
  } catch (error) {
    message = String(error);
    messageKind = "error";
  } finally {
    busy = false;
    render();
  }
}

async function loadCatalog() {
  if (!catalogSource) {
    message = "Set a catalog source in Sources.";
    messageKind = "error";
    render();
    return;
  }
  busy = true;
  message = "Loading campaigns…";
  messageKind = "neutral";
  render();
  try {
    catalog = await invoke<Catalog>("load_catalog", { source: catalogSource });
    selectedId ||= catalog.campaigns[0]?.id ?? "";
    message = catalog.sourceKind === "cached"
      ? "Offline — showing the last available campaign list."
      : "";
    messageKind = catalog.sourceKind === "cached" ? "neutral" : "success";
    await inspectDirectory();
  } catch (error) {
    catalog = null;
    message = String(error);
    messageKind = "error";
  } finally {
    busy = false;
    render();
  }
}

async function inspectDirectory() {
  if (!gameDir) {
    inspection = null;
    savedResumes = [];
    return;
  }
  const [gameInspection, resumes] = await Promise.all([
    invoke<Inspection>("inspect_game_directory", { gameDir, knownCampaigns: catalog?.campaigns ?? [] }),
    invoke<SavedCampaignResume[]>("inspect_saved_campaign_resumes", { gameDir, profileDir: profileDir || null }),
  ]);
  inspection = gameInspection;
  savedResumes = resumes;
  if (inspection.recoveryPerformed) {
    message = "A previously interrupted install was restored safely.";
    messageKind = "success";
  }
}

async function restoreOriginals(targetPath: string) {
  if (!gameDir || !targetPath) return;
  busy = true;
  message = "Restoring original campaign files…";
  messageKind = "neutral";
  render();
  try {
    const result = await invoke<RestoreResult>("restore_original_campaigns", { gameDir, profileDir: profileDir || null, targetPath });
    if (result.conflicts.length) {
      message = `Nothing was changed: ${result.conflicts.length} managed files were modified outside CCM Reborn.`;
      messageKind = "error";
    } else {
      message = result.restoredFiles ? `Original campaign files restored (${result.restoredFiles}).` : "Original campaigns are already active.";
      messageKind = "success";
    }
    await inspectDirectory();
  } catch (error) {
    message = String(error);
    messageKind = "error";
  } finally {
    busy = false;
    render();
  }
}

async function migrateLegacySnapshot(campaignId: string) {
  busy = true;
  message = "Migrating legacy CCM saves to the selected account…";
  messageKind = "neutral";
  render();
  try {
    const result = await migrateLegacyProfile(campaignId, profileDir);
    if (!result) {
      message = "";
      return;
    }
    message = `Migrated ${result.filesCopied} legacy profile files for ${result.campaignId}.`;
    messageKind = "success";
    await inspectDirectory();
  } catch (error) {
    message = String(error);
    messageKind = "error";
  } finally {
    busy = false;
    render();
  }
}

async function openBattleNet() {
  busy = true;
  message = "Opening Battle.net…";
  messageKind = "neutral";
  render();
  try {
    await invoke("open_battle_net");
    const successMessage = "Battle.net is opening. Press Play there to launch StarCraft II.";
    message = successMessage;
    messageKind = "success";
    window.clearTimeout(battleNetMessageTimer);
    battleNetMessageTimer = window.setTimeout(() => {
      if (message === successMessage) {
        message = "";
        render();
      }
    }, 3500);
  } catch (error) {
    message = String(error);
    messageKind = "error";
  } finally {
    busy = false;
    render();
  }
}

async function boot() {
  try {
    diagnosticLogPath = await invoke<string>("get_diagnostic_log_path");
  } catch {
    // The installation flow remains usable when the user's home directory is unavailable.
  }
  if (!gameDir) {
    try {
      const locations = await invoke<GameDirectoryCandidate[]>("detect_game_directories");
      if (locations.length) setGameDirectory(locations[0]);
    } catch {
      // A missing installation is a normal first-run state; the dashboard still opens.
    }
  }
  if (gameDir) await refreshProfiles();
  if (catalogSource) await loadCatalog();
  else render();
}

render();
void listen<InstallProgress>("install-progress", ({ payload }) => {
  if (!busy) return;
  installActivity = payload;
  message = payload.message;
  messageKind = "neutral";
  render();
});
void boot();
window.addEventListener("focus", () => {
  if (!gameDir || busy) return;
  void inspectDirectory().then(render).catch(() => {
    // The previous inspection stays visible if the game directory disappears.
  });
});
