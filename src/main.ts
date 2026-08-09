import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import "./styles.css";
import type {
  Campaign, Catalog, CurrentCampaign, DryRunPlan, GameDirectoryCandidate, Inspection,
  InstallResult, RestoreResult, SavedCampaignResume, StarcraftProfileCandidate,
} from "./types";
import { renderPlanDialog } from "./render-plan";
import { renderLibrary } from "./render-library";
import { campaignSlot, coverClass, formatBytes, isCurrentCatalogCampaign } from "./domain";
import { profileName, resumeFor, resumeInstruction } from "./resume";

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) throw new Error("Application root is missing.");
const root = app;

const slots = [
  { id: "wings-of-liberty", title: "Wings of Liberty", short: "WOL", colour: "ember" },
  { id: "heart-of-the-swarm", title: "Heart of the Swarm", short: "HOTS", colour: "jade" },
  { id: "legacy-of-the-void", title: "Legacy of the Void", short: "LOTV", colour: "void" },
  { id: "nova-covert-ops", title: "Nova Covert Ops", short: "NCO", colour: "arc" },
] as const;

const defaultCatalog = import.meta.env.DEV ? "../dev-catalog/catalog.json" : "";
let catalogSource = localStorage.getItem("ccm-catalog-source") ?? defaultCatalog;
let gameDir = localStorage.getItem("ccm-game-directory") ?? "";
let profileDir = localStorage.getItem("ccm-profile-directory") ?? "";
let profileCandidates: StarcraftProfileCandidate[] = [];
let catalog: Catalog | null = null;
let inspection: Inspection | null = null;
let savedResumes: SavedCampaignResume[] = [];
let page: "dashboard" | "library" = "dashboard";
let selectedId = "";
let message = "Preparing campaign control…";
let messageKind: "neutral" | "success" | "error" = "neutral";
let busy = false;
let launchMessageTimer: number | undefined;
let pendingPlan: DryRunPlan | null = null;

const escapeHtml = (value: string) => value.replace(/[&<>'"]/g, (character) =>
  ({ "&": "&amp;", "<": "&gt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character]!,
);

function render() {
  const catalogLabel = catalog
    ? `${escapeHtml(catalog.name)} · ${catalog.sourceKind === "local" ? "local catalog" : "Cloudflare catalog"}`
    : "Catalog unavailable";
  const locationLabel = gameDir ? escapeHtml(gameDir) : "Searching automatically…";
  const activeProfileLabel = profileDir
    ? `Profile ${escapeHtml(profileName(profileDir))} · this account's saves and campaign progress are in use`
    : "No account profile selected — installs are preview-only";

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
        <div><p class="eyebrow">${catalogLabel}</p><h1>${page === "dashboard" ? "Your campaigns" : "Campaign library"}</h1><p class="subhead">${page === "dashboard" ? `${locationLabel}<br /><span class="profile-context">${activeProfileLabel}</span>` : "Browse every package in the current catalog."}</p></div>
        <div class="header-actions">
          <button class="settings-button" data-action="show-settings">Sources</button>
        </div>
      </header>
      ${message ? `<section class="status ${messageKind}"><span>${messageKind === "success" ? "✓" : messageKind === "error" ? "!" : "i"}</span><p>${escapeHtml(message)}</p></section>` : ""}
      ${page === "dashboard" ? `<section class="campaign-dashboard">${slots.map(renderSlot).join("")}</section>` : renderLibrary({ catalog, inspection, selectedId, busy, gameDir, isCurrentCatalogCampaign: (campaign) => isCurrentCatalogCampaign(campaign, inspection) })}
    </main>
    <dialog id="settings-dialog">
      <form method="dialog" class="dialog-card">
        <button class="close" value="cancel" aria-label="Close">×</button>
        <p class="eyebrow">SOURCES</p><h2>Catalog & game location</h2>
        <label>Catalog source
          <input id="catalog-source" spellcheck="false" value="${escapeHtml(catalogSource)}" placeholder="/path/to/catalog.json or https://…/catalog.json" />
          <small>Local development reads <code>catalog.json</code>; production can point to a Cloudflare HTTPS URL.</small>
        </label>
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

function renderSlot(slot: typeof slots[number]) {
  const current = inspection?.activeCampaigns.find((campaign) => campaign.slot === slot.id);
  const options = catalog?.campaigns.filter((campaign) => campaignSlot(campaign.requirements.campaign) === slot.id && !isCurrentCatalogCampaign(campaign, inspection)) ?? [];
  // A package can only be "current" for its own campaign slot.  Looking it
  // up across the whole catalog made the active HotS package leak its resume
  // and Repair button into WoL/ LotV cards.
  const currentPackage = catalog?.campaigns.find((campaign) =>
    campaignSlot(campaign.requirements.campaign) === slot.id && isCurrentCatalogCampaign(campaign, inspection)
  );
  const currentTitle = current?.title ?? "StarCraft II directory not selected";
  const currentMeta = current ? `${current.author} · ${current.version}` : "Detect the game directory to inspect its active campaign.";
  const managed = inspection?.managedCampaigns.find((campaign) => campaign.slot === slot.id);
  const managedHere = Boolean(managed && catalog?.campaigns.some((campaign) => campaign.id === managed.id));
  // The managed state records the exact directory it owns. That is the
  // authoritative slot; title/version matching against the catalog is only
  // presentation and may change after an update.
  const activeInstalled = Boolean(managed);
  const activeResume = managed ? resumeFor(savedResumes, managed.id) : null;

  return `
    <article class="campaign-slot ${slot.colour}">
      <header class="slot-header">
        <div class="slot-sigil">${slot.short.slice(0, 1)}</div>
        <div><p class="eyebrow">${slot.short}</p><h2>${slot.title}</h2></div>
        <span class="slot-state ${current?.isModified ? "custom" : "original"}">${current?.isModified ? "CUSTOM" : "ORIGINAL / UNKNOWN"}</span>
      </header>
      <section class="current-install">
        <div><small>CURRENTLY INSTALLED</small><strong>${escapeHtml(currentTitle)}</strong><span>${escapeHtml(currentMeta)}</span>${activeInstalled ? `<p class="resume-instruction"><small>${activeResume?.latestSave ? "CCM RESUME — DO NOT USE CLOUD CONTINUE" : "CCM START NEW CAMPAIGN"}</small>${escapeHtml(resumeInstruction(activeResume))}</p>` : ""}</div>
        <div class="current-slot-actions"><button class="ghost play" data-action="play" ${!inspection?.canLaunch || busy ? "disabled" : ""}>Play current</button>${currentPackage ? `<button class="ghost repair" data-action="install" data-campaign="${escapeHtml(currentPackage.id)}" ${!gameDir || busy ? "disabled" : ""}>Repair</button>` : ""}${managed ? `<button class="ghost repair" data-action="restore" data-target="${escapeHtml(managed.targetPath)}" ${!gameDir || busy || !profileDir ? "disabled" : ""}>Restore original</button>` : ""}</div>
      </section>
      <section class="alternatives">
        <div class="alternative-heading"><small>INSTALL SOMETHING ELSE</small><span>${options.length} available</span></div>
        ${options.length ? options.map((campaign) => `
          <article class="install-option">
            <div class="option-cover cover-${coverClass(campaign.id)}">${escapeHtml(campaign.title.slice(0, 1).toUpperCase())}</div>
            <div class="option-copy"><strong>${escapeHtml(campaign.title)}</strong><span>by ${escapeHtml(campaign.author)} · v${escapeHtml(campaign.version)} · ${formatBytes(campaign.package.size)}</span></div>
            <button class="primary compact" data-action="install" data-campaign="${escapeHtml(campaign.id)}" ${!gameDir || busy ? "disabled" : ""}>Install</button>
          </article>
        `).join("") : '<p class="no-options">No packages for this campaign in the current catalog.</p>'}
      </section>
      ${managedHere ? '<p class="managed-note">CCM Reborn has a restorable snapshot for this active change.</p>' : ""}
    </article>`;
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
    catalogSource = document.querySelector<HTMLInputElement>("#catalog-source")?.value.trim() ?? "";
    profileDir = document.querySelector<HTMLSelectElement>("#profile-directory")?.value.trim() ?? profileDir;
    localStorage.setItem("ccm-catalog-source", catalogSource);
    if (profileDir) localStorage.setItem("ccm-profile-directory", profileDir);
    else localStorage.removeItem("ccm-profile-directory");
    document.querySelector<HTMLDialogElement>("#settings-dialog")?.close();
    void loadCatalog();
  });
  document.querySelector<HTMLButtonElement>("[data-action='choose-directory']")?.addEventListener("click", () => void chooseGameDirectory());
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
  message = `StarCraft II profile selected: ${profileDir}`;
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
  message = "Loading catalog…";
  messageKind = "neutral";
  render();
  try {
    catalog = await invoke<Catalog>("load_catalog", { source: catalogSource });
    selectedId ||= catalog.campaigns[0]?.id ?? "";
    message = `${catalog.campaigns.length} packages loaded. Select a campaign branch to install a replacement.`;
    messageKind = "success";
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
