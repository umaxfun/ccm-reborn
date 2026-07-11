import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import "./styles.css";

type Campaign = {
  id: string;
  title: string;
  author: string;
  version: string;
  description: string;
  tags: string[];
  requirements: { campaign: string; platforms: string[] };
  package: { source: string; sha256: string; size: number };
};

type Catalog = {
  name: string;
  updatedAt: string;
  sourceKind: "local" | "remote";
  campaigns: Campaign[];
};

type CurrentCampaign = {
  slot: string;
  campaign: string;
  title: string;
  author: string;
  version: string;
  isModified: boolean;
};

type Inspection = {
  exists: boolean;
  path: string;
  activeCampaign: { id: string; title: string; files: number } | null;
  activeCampaigns: CurrentCampaign[];
  canLaunch: boolean;
  recoveryPerformed: boolean;
};

type GameDirectoryCandidate = { path: string; label: string };
type RestoreResult = { restoredFiles: number; conflicts: string[] };

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
let catalog: Catalog | null = null;
let inspection: Inspection | null = null;
let page: "dashboard" | "library" = "dashboard";
let selectedId = "";
let message = "Preparing campaign control…";
let messageKind: "neutral" | "success" | "error" = "neutral";
let busy = false;
let launchMessageTimer: number | undefined;

const escapeHtml = (value: string) => value.replace(/[&<>'"]/g, (character) =>
  ({ "&": "&amp;", "<": "&gt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character]!,
);

const formatBytes = (bytes: number) => bytes < 1024 * 1024
  ? `${Math.max(1, Math.round(bytes / 1024))} KB`
  : `${(bytes / 1024 / 1024).toFixed(bytes > 1024 * 1024 * 1024 ? 1 : 0)} MB`;

function campaignSlot(campaign: string) {
  const value = campaign.toLowerCase();
  if (value.includes("wings") || value.includes("liberty") || value.includes("wol")) return "wings-of-liberty";
  if (value.includes("heart") || value.includes("swarm") || value.includes("hots")) return "heart-of-the-swarm";
  if (value.includes("legacy") || value.includes("void") || value.includes("lotv")) return "legacy-of-the-void";
  return "nova-covert-ops";
}

function isCurrentCatalogCampaign(campaign: Campaign) {
  const current = inspection?.activeCampaigns.find((item) => item.slot === campaignSlot(campaign.requirements.campaign));
  return Boolean(
    current
      && current.title.trim().toLocaleLowerCase() === campaign.title.trim().toLocaleLowerCase()
      && current.version.trim() === campaign.version.trim(),
  );
}

function render() {
  const catalogLabel = catalog
    ? `${escapeHtml(catalog.name)} · ${catalog.sourceKind === "local" ? "local catalog" : "Cloudflare catalog"}`
    : "Catalog unavailable";
  const locationLabel = gameDir ? escapeHtml(gameDir) : "Searching automatically…";

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
        <div><p class="eyebrow">${catalogLabel}</p><h1>${page === "dashboard" ? "Your campaigns" : "Campaign library"}</h1><p class="subhead">${page === "dashboard" ? locationLabel : "Browse every package in the current catalog."}</p></div>
        <div class="header-actions">
          ${inspection?.activeCampaign ? '<button class="ghost" data-action="restore" ' + (busy ? "disabled" : "") + '>Restore last change</button>' : ""}
          <button class="settings-button" data-action="show-settings">Sources</button>
        </div>
      </header>
      ${message ? `<section class="status ${messageKind}"><span>${messageKind === "success" ? "✓" : messageKind === "error" ? "!" : "i"}</span><p>${escapeHtml(message)}</p></section>` : ""}
      ${page === "dashboard" ? `<section class="campaign-dashboard">${slots.map(renderSlot).join("")}</section>` : renderLibrary()}
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
        <div class="dialog-actions">
          <button class="ghost" value="cancel">Cancel</button>
          <button class="primary" value="default" data-action="save-settings">Reload catalog</button>
        </div>
      </form>
    </dialog>
  `;
  bindEvents();
}

function renderLibrary() {
  const selected = catalog?.campaigns.find((campaign) => campaign.id === selectedId) ?? catalog?.campaigns[0] ?? null;
  return `
    <section class="workspace library-workspace">
      <div class="campaign-list">
        ${catalog?.campaigns.length ? catalog.campaigns.map((campaign) => `
          <button class="campaign-card ${campaign.id === selected?.id ? "selected" : ""}" data-library-campaign="${escapeHtml(campaign.id)}">
            <div class="cover cover-${coverClass(campaign.id)}"><span>${escapeHtml(campaign.title.slice(0, 1).toUpperCase())}</span></div>
            <div class="campaign-copy"><h2>${escapeHtml(campaign.title)}</h2><p>by ${escapeHtml(campaign.author)} · v${escapeHtml(campaign.version)}</p></div>
            ${isCurrentCatalogCampaign(campaign) ? '<span class="installed">CURRENT</span>' : ""}
          </button>
        `).join("") : '<div class="empty-list">No campaigns in this catalog yet.</div>'}
      </div>
      <article class="campaign-detail">${selected ? renderLibraryDetail(selected) : '<div class="empty-detail"><div class="empty-orb">◇</div><h2>Library is empty.</h2></div>'}</article>
    </section>`;
}

function renderLibraryDetail(campaign: Campaign) {
  const slot = slots.find((item) => item.id === campaignSlot(campaign.requirements.campaign));
  const current = inspection?.activeCampaigns.find((item) => item.slot === slot?.id);
  const isCurrent = isCurrentCatalogCampaign(campaign);
  return `
    <div class="hero cover-${coverClass(campaign.id)}"><div class="grid"></div><span class="hero-label">${escapeHtml(campaign.requirements.campaign)}</span><div class="hero-rune">${escapeHtml(campaign.title.slice(0, 1).toUpperCase())}</div></div>
    <div class="detail-body">
      <div class="title-row"><div><p class="eyebrow">${escapeHtml(campaign.author.toUpperCase())}</p><h2>${escapeHtml(campaign.title)}</h2></div><span class="version">v${escapeHtml(campaign.version)}</span></div>
      <p class="description">${escapeHtml(campaign.description)}</p>
      <div class="metadata"><div><small>PACKAGE</small><strong>${formatBytes(campaign.package.size)}</strong></div><div><small>BRANCH</small><strong>${escapeHtml(campaign.requirements.campaign)}</strong></div><div><small>INTEGRITY</small><strong>SHA-256 verified</strong></div></div>
      <div class="tags">${campaign.tags.map((tag) => `<span>${escapeHtml(tag)}</span>`).join("")}</div>
      <section class="current-campaign"><div><small>CURRENT ${escapeHtml(campaign.requirements.campaign.toUpperCase())}</small><strong>${current ? escapeHtml(current.title) : "StarCraft II directory not selected"}</strong><span>${current ? `${escapeHtml(current.author)} · ${escapeHtml(current.version)}` : "Auto-detection runs when CCM Reborn starts."}</span></div><button class="ghost play" data-action="play" ${!inspection?.canLaunch || busy ? "disabled" : ""}>Play current</button></section>
      <div class="install-row">${isCurrent ? `<button class="ghost" data-action="install" data-campaign="${escapeHtml(campaign.id)}" ${!gameDir || busy ? "disabled" : ""}>Repair installation</button>` : `<button class="primary install" data-action="install" data-campaign="${escapeHtml(campaign.id)}" ${!gameDir || busy ? "disabled" : ""}>Install v${escapeHtml(campaign.version)}</button>`}</div>
    </div>`;
}

function renderSlot(slot: typeof slots[number]) {
  const current = inspection?.activeCampaigns.find((campaign) => campaign.slot === slot.id);
  const options = catalog?.campaigns.filter((campaign) => campaignSlot(campaign.requirements.campaign) === slot.id && !isCurrentCatalogCampaign(campaign)) ?? [];
  const currentPackage = catalog?.campaigns.find((campaign) => isCurrentCatalogCampaign(campaign));
  const currentTitle = current?.title ?? "StarCraft II directory not selected";
  const currentMeta = current ? `${current.author} · ${current.version}` : "Detect the game directory to inspect its active campaign.";
  const managedHere = inspection?.activeCampaign && options.some((campaign) => campaign.id === inspection?.activeCampaign?.id);

  return `
    <article class="campaign-slot ${slot.colour}">
      <header class="slot-header">
        <div class="slot-sigil">${slot.short.slice(0, 1)}</div>
        <div><p class="eyebrow">${slot.short}</p><h2>${slot.title}</h2></div>
        <span class="slot-state ${current?.isModified ? "custom" : "original"}">${current?.isModified ? "CUSTOM" : "ORIGINAL / UNKNOWN"}</span>
      </header>
      <section class="current-install">
        <div><small>CURRENTLY INSTALLED</small><strong>${escapeHtml(currentTitle)}</strong><span>${escapeHtml(currentMeta)}</span></div>
        <div class="current-slot-actions"><button class="ghost play" data-action="play" ${!inspection?.canLaunch || busy ? "disabled" : ""}>Play current</button>${currentPackage ? `<button class="ghost repair" data-action="install" data-campaign="${escapeHtml(currentPackage.id)}" ${!gameDir || busy ? "disabled" : ""}>Repair</button>` : ""}</div>
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

function coverClass(id: string) {
  return ["ember", "void", "arc", "jade"][Array.from(id).reduce((total, character) => total + character.charCodeAt(0), 0) % 4];
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
    localStorage.setItem("ccm-catalog-source", catalogSource);
    document.querySelector<HTMLDialogElement>("#settings-dialog")?.close();
    void loadCatalog();
  });
  document.querySelector<HTMLButtonElement>("[data-action='choose-directory']")?.addEventListener("click", () => void chooseGameDirectory());
  document.querySelector<HTMLButtonElement>("[data-action='detect-directory']")?.addEventListener("click", () => void detectGameDirectory());
  document.querySelectorAll<HTMLButtonElement>("[data-action='install']").forEach((button) => {
    button.addEventListener("click", () => void installCampaign(button.dataset.campaign ?? ""));
  });
  document.querySelectorAll<HTMLButtonElement>("[data-action='play']").forEach((button) => button.addEventListener("click", () => void playCurrentCampaign()));
  document.querySelector<HTMLButtonElement>("[data-action='restore']")?.addEventListener("click", () => void restoreOriginals());
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
    message = `StarCraft II directory selected: ${game.label}`;
    messageKind = "success";
    await inspectDirectory();
  } catch (error) {
    message = String(error);
    messageKind = "error";
  }
  render();
}

function setGameDirectory(game: GameDirectoryCandidate) {
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
    return;
  }
  inspection = await invoke<Inspection>("inspect_game_directory", { gameDir, knownCampaigns: catalog?.campaigns ?? [] });
  if (inspection.recoveryPerformed) {
    message = "A previously interrupted install was restored safely.";
    messageKind = "success";
  }
}

async function installCampaign(campaignId: string) {
  const campaign = catalog?.campaigns.find((item) => item.id === campaignId);
  if (!campaign || !gameDir) return;
  busy = true;
  message = `Installing ${campaign.title}…`;
  messageKind = "neutral";
  render();
  try {
    const result = await invoke<{ filesInstalled: number }>("install_campaign", {
      request: { campaignId: campaign.id, title: campaign.title, archiveSource: campaign.package.source, sha256: campaign.package.sha256, gameDir },
    });
    message = `${campaign.title} installed — ${result.filesInstalled} managed files.`;
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

async function restoreOriginals() {
  if (!gameDir) return;
  busy = true;
  message = "Restoring original campaign files…";
  messageKind = "neutral";
  render();
  try {
    const result = await invoke<RestoreResult>("restore_original_campaigns", { gameDir });
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
    message = result.message;
    messageKind = "success";
    window.clearTimeout(launchMessageTimer);
    launchMessageTimer = window.setTimeout(() => {
      if (message === result.message) {
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
  if (catalogSource) await loadCatalog();
  else render();
}

render();
void boot();
