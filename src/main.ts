import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import "./styles.css";
import type {
  Catalog, DryRunPlan, GameDirectoryCandidate, Inspection,
  InstallResult, RestoreResult, SavedCampaignResume, StarcraftProfileCandidate,
} from "./types";
import { renderPlanDialog } from "./render-plan";
import { renderLibrary } from "./render-library";
import { isCurrentCatalogCampaign } from "./domain";
import { profileName, resumeFor } from "./resume";
import { migrateLegacyProfile, renderLegacyMigration } from "./legacy-migration";
import { renderDashboard } from "./render-dashboard";

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
let launchMessageTimer: number | undefined;
let pendingPlan: DryRunPlan | null = null;

const escapeHtml = (value: string) => value.replace(/[&<>'"]/g, (character) =>
  ({ "&": "&amp;", "<": "&gt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character]!,
);

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
      ${message ? `<section class="status ${messageKind}"><span>${messageKind === "success" ? "✓" : messageKind === "error" ? "!" : "i"}</span><p>${escapeHtml(message)}</p></section>` : ""}
      ${renderLegacyMigration(catalog, savedResumes, profileDir, busy)}
      ${page === "dashboard" ? renderDashboard({ catalog, inspection, savedResumes, busy, gameDir, profileDir, isCurrentCatalogCampaign: (campaign) => isCurrentCatalogCampaign(campaign, inspection) }) : renderLibrary({ catalog, inspection, selectedId, busy, gameDir, isCurrentCatalogCampaign: (campaign) => isCurrentCatalogCampaign(campaign, inspection) })}
    </main>
    <dialog id="settings-dialog">
      <form method="dialog" class="dialog-card">
        <button class="close" value="cancel" aria-label="Close">×</button>
        <p class="eyebrow">SOURCES</p><h2>Catalog & game location</h2>
        <label>Catalog source
          <input id="catalog-source" spellcheck="false" value="${escapeHtml(catalogSource)}" placeholder="/path/to/catalog.json or https://…/catalog.json" />
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
            ${profileCandidates.map((profile, index) => `<option value="${escapeHtml(profile.path)}" ${profile.path === profileDir ? "selected" : ""}>Profile ${escapeHtml(profileName(profile.path))}${index === 0 ? " · most recently active" : ""} — ${escapeHtml(profile.label)}</option>`).join("")}
          </select>
          <small>This is the single account profile whose bank, saves, and campaign progress CCM may switch. The first detected profile is selected automatically; choose the other one only if that is the account you play on.</small>
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
  const planDialog = document.querySelector<HTMLDialogElement>("#plan-dialog");
  if (planDialog && !planDialog.open) planDialog.showModal();
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
    button.addEventListener("click", () => document.querySelector<HTMLDialogElement>("#settings-dialog")?.showModal());
  });
  document.querySelector<HTMLButtonElement>("[data-action='save-settings']")?.addEventListener("click", (event) => {
    event.preventDefault();
    catalogSource = document.querySelector<HTMLInputElement>("#catalog-source")?.value.trim() || defaultCatalog;
    profileDir = document.querySelector<HTMLSelectElement>("#profile-directory")?.value.trim() ?? profileDir;
    localStorage.setItem("ccm-catalog-source", catalogSource);
    if (profileDir) localStorage.setItem("ccm-profile-directory", profileDir);
    else localStorage.removeItem("ccm-profile-directory");
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
    button.addEventListener("click", () => void installCampaign(button.dataset.campaign ?? ""));
  });
  document.querySelectorAll<HTMLButtonElement>("[data-action='close-plan']").forEach((button) => {
    button.addEventListener("click", () => {
      pendingPlan = null;
      render();
    });
  });
  document.querySelector<HTMLButtonElement>("[data-action='apply-plan']")?.addEventListener("click", () => void applyPendingPlan());
  document.querySelectorAll<HTMLButtonElement>("[data-action='play']").forEach((button) => button.addEventListener("click", () => void playCurrentCampaign()));
  document.querySelectorAll<HTMLButtonElement>("[data-action='restore']").forEach((button) => {
    button.addEventListener("click", () => void restoreOriginals(button.dataset.target ?? ""));
  });
  document.querySelectorAll<HTMLButtonElement>("[data-action='migrate-legacy']").forEach((button) => {
    button.addEventListener("click", () => void migrateLegacySnapshot(button.dataset.campaign ?? ""));
  });
}

function useCatalogSource(source: string) {
  catalogSource = source;
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

async function installCampaign(campaignId: string) {
  const campaign = catalog?.campaigns.find((item) => item.id === campaignId);
  if (!campaign || !gameDir) return;
  busy = true;
  pendingPlan = null;
  message = `Planning a safe dry-run for ${campaign.title}…`;
  messageKind = "neutral";
  render();
  try {
    const result = await invoke<DryRunPlan>("plan_campaign_install", {
      request: { campaignId: campaign.id, title: campaign.title, author: campaign.author, version: campaign.version, profileDir: profileDir || null, archiveSource: campaign.package.source, sha256: campaign.package.sha256, packageSize: campaign.package.size, gameDir },
    });
    pendingPlan = result;
    message = `Dry-run complete for ${campaign.title}. No files were changed.`;
    messageKind = "success";
  } catch (error) {
    message = String(error);
    messageKind = "error";
  } finally {
    busy = false;
    render();
  }
}

async function applyPendingPlan() {
  if (!pendingPlan || !gameDir || !profileDir) return;
  const campaign = catalog?.campaigns.find((item) => item.id === pendingPlan?.campaignId);
  if (!campaign) return;
  const confirmed = window.confirm(
    `Apply ${campaign.title} v${campaign.version}? StarCraft II must be fully closed. CCM will use only the selected profile and keep rollback snapshots for both profile and campaign files.`,
  );
  if (!confirmed) return;
  busy = true;
  pendingPlan = null;
  message = `Applying ${campaign.title}; staging profile and campaign changes…`;
  messageKind = "neutral";
  render();
  try {
    const result = await invoke<InstallResult>("install_campaign", {
      request: {
        campaignId: campaign.id,
        title: campaign.title,
        author: campaign.author,
        version: campaign.version,
        profileDir,
        archiveSource: campaign.package.source,
        sha256: campaign.package.sha256,
        packageSize: campaign.package.size,
        gameDir,
      },
    });
    message = `Installed ${result.title} v${result.version} (${result.filesInstalled} files).`;
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

async function playCurrentCampaign() {
  if (!gameDir) return;
  busy = true;
  message = "Launching StarCraft II…";
  messageKind = "neutral";
  render();
  try {
    const result = await invoke<{ message: string }>("launch_current_campaign", { gameDir });
    const resume = resumeFor(savedResumes, inspection?.activeCampaign?.id);
    const launchMessage = resume?.latestSave
      ? `SC2 launched. Do not use cloud Continue; choose Load and open ${resume.latestSave.relativePath}.`
      : inspection?.activeCampaign
        ? "SC2 launched. Start a New Campaign; cloud Continue belongs to the Battle.net profile, not this mod."
        : result.message;
    message = launchMessage;
    messageKind = "success";
    window.clearTimeout(launchMessageTimer);
    launchMessageTimer = window.setTimeout(() => {
      if (message === launchMessage) {
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
void boot();
window.addEventListener("focus", () => {
  if (!gameDir || busy) return;
  void inspectDirectory().then(render).catch(() => {
    // The previous inspection stays visible if the game directory disappears.
  });
});
