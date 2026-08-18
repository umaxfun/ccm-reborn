import type { Campaign, Catalog, Inspection, SavedCampaignResume } from "./types";
import { campaignSlot, coverClass, formatBytes } from "./domain";
import { resumeCheckpoint, resumeFor, resumeInstruction, resumeSummary, sortCampaignsByRecentPlay } from "./resume";

const slots = [
  { id: "wings-of-liberty", title: "Wings of Liberty", short: "WOL", colour: "ember" },
  { id: "heart-of-the-swarm", title: "Heart of the Swarm", short: "HOTS", colour: "jade" },
  { id: "legacy-of-the-void", title: "Legacy of the Void", short: "LOTV", colour: "void" },
  { id: "nova-covert-ops", title: "Nova Covert Ops", short: "NCO", colour: "arc" },
] as const;

const escapeHtml = (value: string) => value.replace(/[&<>'"]/g, (character) =>
  ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character]!,
);

type DashboardContext = {
  catalog: Catalog | null;
  inspection: Inspection | null;
  savedResumes: SavedCampaignResume[];
  busy: boolean;
  gameDir: string;
  profileDir: string;
  installingCampaignId: string;
  highlightCampaignId: string;
  isCurrentCatalogCampaign: (campaign: Campaign) => boolean;
};

export function renderDashboard(context: DashboardContext) {
  return `<section class="campaign-dashboard">${slots.map((slot) => renderSlot(slot, context)).join("")}</section>`;
}

function renderSlot(slot: typeof slots[number], context: DashboardContext) {
  const current = context.inspection?.activeCampaigns.find((campaign) => campaign.slot === slot.id);
  const options = sortCampaignsByRecentPlay(
    context.catalog?.campaigns.filter((campaign) => campaignSlot(campaign.requirements.campaign) === slot.id) ?? [],
    context.savedResumes,
  );
  const currentPackage = context.catalog?.campaigns.find((campaign) =>
    campaignSlot(campaign.requirements.campaign) === slot.id && context.isCurrentCatalogCampaign(campaign)
  );
  const currentTitle = current?.title ?? "StarCraft II directory not selected";
  const currentMeta = current ? `Version ${current.version}` : "Choose the StarCraft II folder to see what is installed.";
  const managed = context.inspection?.managedCampaigns.find((campaign) => campaign.slot === slot.id);
  const activeResume = managed ? resumeFor(context.savedResumes, managed.id) : null;

  return `
    <article class="campaign-slot ${slot.colour}">
      <header class="slot-header">
        <div class="slot-sigil">${slot.short.slice(0, 1)}</div>
        <div><p class="eyebrow">${slot.short}</p><h2>${slot.title}</h2></div>
        <span class="slot-state ${current?.isModified ? "custom" : "original"}">${current?.isModified ? "Custom" : current ? "Original" : "Not detected"}</span>
      </header>
      <section class="current-install">
        <div class="current-campaign-copy"><small>PLAYING NOW</small><strong>${escapeHtml(currentTitle)}</strong><span>${escapeHtml(currentMeta)}</span>${managed ? `<div class="resume-instruction"><strong>${activeResume?.latestSave ? `Continue from ${escapeHtml(resumeCheckpoint(activeResume) ?? "your last save")}` : "Ready to start"}</strong><span>${escapeHtml(resumeInstruction(activeResume))}</span></div>` : ""}</div>
        <div class="current-slot-actions"><button class="primary compact open-battle-net" data-action="open-battle-net" ${context.busy ? "disabled" : ""}>Open Battle.net</button>${currentPackage || managed ? `<details class="campaign-options"><summary>More</summary><div>${currentPackage ? `<button class="ghost repair" data-action="install" data-campaign="${escapeHtml(currentPackage.id)}" ${!context.gameDir || context.busy ? "disabled" : ""}>Repair files</button>` : ""}${managed ? `<button class="ghost repair" data-action="restore" data-target="${escapeHtml(managed.targetPath)}" ${!context.gameDir || context.busy || !context.profileDir ? "disabled" : ""}>Restore original</button>` : ""}</div></details>` : ""}</div>
      </section>
      <section class="alternatives">
        <div class="alternative-heading"><small>INSTALL SOMETHING ELSE</small><span>${options.length} available</span></div>
        ${options.length ? options.map((campaign) => {
          const isCurrent = context.isCurrentCatalogCampaign(campaign);
          return `
          <article class="install-option${campaign.isLocal ? " local" : ""}${campaign.id === context.highlightCampaignId ? " highlighted" : ""}">
            <div class="option-cover cover-${coverClass(campaign.id)}">${escapeHtml(campaign.title.slice(0, 1).toUpperCase())}</div>
            <div class="option-copy"><strong><span class="option-title">${escapeHtml(campaign.title)}</span>${campaign.isLocal ? '<span class="local-pill">LOCAL</span>' : ""}</strong><span>by ${escapeHtml(campaign.author)} · v${escapeHtml(campaign.version)} · ${formatBytes(campaign.package.size)}</span><span class="option-progress">${escapeHtml(resumeSummary(resumeFor(context.savedResumes, campaign.id)))}</span>${campaign.isLocal ? `<span class="option-remove"><button type="button" class="link-action" data-action="remove-local-mod" data-campaign="${escapeHtml(campaign.id)}" ${context.busy ? "disabled" : ""}>Remove from list</button></span>` : ""}</div>
            ${isCurrent
              ? `<button class="primary compact open-battle-net" data-action="open-battle-net" ${context.busy ? "disabled" : ""}>Open Battle.net</button>`
              : `<button class="primary compact" data-action="install" data-campaign="${escapeHtml(campaign.id)}" ${!context.gameDir || context.busy ? "disabled" : ""}>${context.busy && context.installingCampaignId === campaign.id ? "Working…" : "Install"}</button>`}
          </article>
        `;
        }).join("") : '<p class="no-options">No packages for this campaign yet — check for updates or add a mod from your computer.</p>'}
      </section>
    </article>`;
}
