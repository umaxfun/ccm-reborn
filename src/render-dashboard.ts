import type { Campaign, Catalog, Inspection, SavedCampaignResume } from "./types";
import { campaignSlot, coverClass, formatBytes } from "./domain";
import { resumeFor, resumeInstruction, resumeSummary, sortCampaignsByRecentPlay } from "./resume";

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
  isCurrentCatalogCampaign: (campaign: Campaign) => boolean;
};

export function renderDashboard(context: DashboardContext) {
  return `<section class="campaign-dashboard">${slots.map((slot) => renderSlot(slot, context)).join("")}</section>`;
}

function renderSlot(slot: typeof slots[number], context: DashboardContext) {
  const current = context.inspection?.activeCampaigns.find((campaign) => campaign.slot === slot.id);
  const options = sortCampaignsByRecentPlay(
    context.catalog?.campaigns.filter((campaign) => campaignSlot(campaign.requirements.campaign) === slot.id && !context.isCurrentCatalogCampaign(campaign)) ?? [],
    context.savedResumes,
  );
  const currentPackage = context.catalog?.campaigns.find((campaign) =>
    campaignSlot(campaign.requirements.campaign) === slot.id && context.isCurrentCatalogCampaign(campaign)
  );
  const currentTitle = current?.title ?? "StarCraft II directory not selected";
  const currentMeta = current ? `${current.author} · ${current.version}` : "Detect the game directory to inspect its active campaign.";
  const managed = context.inspection?.managedCampaigns.find((campaign) => campaign.slot === slot.id);
  const managedHere = Boolean(managed && context.catalog?.campaigns.some((campaign) => campaign.id === managed.id));
  const activeResume = managed ? resumeFor(context.savedResumes, managed.id) : null;

  return `
    <article class="campaign-slot ${slot.colour}">
      <header class="slot-header">
        <div class="slot-sigil">${slot.short.slice(0, 1)}</div>
        <div><p class="eyebrow">${slot.short}</p><h2>${slot.title}</h2></div>
        <span class="slot-state ${current?.isModified ? "custom" : "original"}">${current?.isModified ? "CUSTOM" : "ORIGINAL / UNKNOWN"}</span>
      </header>
      <section class="current-install">
        <div><small>CURRENTLY INSTALLED</small><strong>${escapeHtml(currentTitle)}</strong><span>${escapeHtml(currentMeta)}</span>${managed ? `<p class="resume-instruction"><small>${activeResume?.latestSave ? "CCM RESUME — DO NOT USE CLOUD CONTINUE" : "CCM START NEW CAMPAIGN"}</small>${escapeHtml(resumeInstruction(activeResume))}</p>` : ""}</div>
        <div class="current-slot-actions"><button class="ghost play" data-action="play" ${!context.inspection?.canLaunch || context.busy ? "disabled" : ""}>Play current</button>${currentPackage ? `<button class="ghost repair" data-action="install" data-campaign="${escapeHtml(currentPackage.id)}" ${!context.gameDir || context.busy ? "disabled" : ""}>Repair</button>` : ""}${managed ? `<button class="ghost repair" data-action="restore" data-target="${escapeHtml(managed.targetPath)}" ${!context.gameDir || context.busy || !context.profileDir ? "disabled" : ""}>Restore original</button>` : ""}</div>
      </section>
      <section class="alternatives">
        <div class="alternative-heading"><small>INSTALL SOMETHING ELSE</small><span>${options.length} available</span></div>
        ${options.length ? options.map((campaign) => `
          <article class="install-option">
            <div class="option-cover cover-${coverClass(campaign.id)}">${escapeHtml(campaign.title.slice(0, 1).toUpperCase())}</div>
            <div class="option-copy"><strong>${escapeHtml(campaign.title)}</strong><span>by ${escapeHtml(campaign.author)} · v${escapeHtml(campaign.version)} · ${formatBytes(campaign.package.size)}</span><span class="option-progress">${escapeHtml(resumeSummary(resumeFor(context.savedResumes, campaign.id)))}</span></div>
            <button class="primary compact" data-action="install" data-campaign="${escapeHtml(campaign.id)}" ${!context.gameDir || context.busy ? "disabled" : ""}>Install</button>
          </article>
        `).join("") : '<p class="no-options">No packages for this campaign in the current catalog.</p>'}
      </section>
      ${managedHere ? '<p class="managed-note">CCM Reborn has a restorable snapshot for this active change.</p>' : ""}
    </article>`;
}
