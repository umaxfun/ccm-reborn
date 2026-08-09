import type { Campaign, Catalog, CurrentCampaign, Inspection } from "./types";

const escapeHtml = (value: string) => value.replace(/[&<>'"]/g, (character) =>
  ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character]!,
);

const formatBytes = (bytes: number) => bytes < 1024 * 1024
  ? `${Math.max(1, Math.round(bytes / 1024))} KB`
  : `${(bytes / 1024 / 1024).toFixed(bytes > 1024 * 1024 * 1024 ? 1 : 0)} MB`;

const slots = [
  { id: "wings-of-liberty" }, { id: "heart-of-the-swarm" },
  { id: "legacy-of-the-void" }, { id: "nova-covert-ops" },
];

function campaignSlot(campaign: string) {
  const value = campaign.toLowerCase();
  if (value.includes("wings") || value.includes("liberty") || value.includes("wol")) return "wings-of-liberty";
  if (value.includes("heart") || value.includes("swarm") || value.includes("hots")) return "heart-of-the-swarm";
  if (value.includes("legacy") || value.includes("void") || value.includes("lotv")) return "legacy-of-the-void";
  return "nova-covert-ops";
}

function coverClass(id: string) {
  return ["ember", "void", "arc", "jade"][Array.from(id).reduce((total, character) => total + character.charCodeAt(0), 0) % 4];
}

export type LibraryContext = {
  catalog: Catalog | null;
  inspection: Inspection | null;
  selectedId: string;
  busy: boolean;
  gameDir: string;
  isCurrentCatalogCampaign: (campaign: Campaign) => boolean;
};

function renderLibraryDetail(campaign: Campaign, context: LibraryContext) {
  const slot = slots.find((item) => item.id === campaignSlot(campaign.requirements.campaign));
  const current = context.inspection?.activeCampaigns.find((item: CurrentCampaign) => item.slot === slot?.id);
  const isCurrent = context.isCurrentCatalogCampaign(campaign);
  return `
    <div class="hero cover-${coverClass(campaign.id)}"><div class="grid"></div><span class="hero-label">${escapeHtml(campaign.requirements.campaign)}</span><div class="hero-rune">${escapeHtml(campaign.title.slice(0, 1).toUpperCase())}</div></div>
    <div class="detail-body"><div class="title-row"><div><p class="eyebrow">${escapeHtml(campaign.author.toUpperCase())}</p><h2>${escapeHtml(campaign.title)}</h2></div><span class="version">v${escapeHtml(campaign.version)}</span></div>
      <p class="description">${escapeHtml(campaign.description)}</p><div class="metadata"><div><small>PACKAGE</small><strong>${formatBytes(campaign.package.size)}</strong></div><div><small>BRANCH</small><strong>${escapeHtml(campaign.requirements.campaign)}</strong></div><div><small>INTEGRITY</small><strong>SHA-256 verified</strong></div></div>
      <div class="tags">${campaign.tags.map((tag) => `<span>${escapeHtml(tag)}</span>`).join("")}</div>
      <section class="current-campaign"><div><small>CURRENT ${escapeHtml(campaign.requirements.campaign.toUpperCase())}</small><strong>${current ? escapeHtml(current.title) : "StarCraft II directory not selected"}</strong><span>${current ? `${escapeHtml(current.author)} · ${escapeHtml(current.version)}` : "Auto-detection runs when CCM Reborn starts."}</span></div><button class="ghost play" data-action="play" ${!context.inspection?.canLaunch || context.busy ? "disabled" : ""}>Play current</button></section>
      <div class="install-row">${isCurrent ? `<button class="ghost" data-action="install" data-campaign="${escapeHtml(campaign.id)}" ${!context.gameDir || context.busy ? "disabled" : ""}>Repair installation</button>` : `<button class="primary install" data-action="install" data-campaign="${escapeHtml(campaign.id)}" ${!context.gameDir || context.busy ? "disabled" : ""}>Install v${escapeHtml(campaign.version)}</button>`}</div></div>`;
}

export function renderLibrary(context: LibraryContext) {
  const selected = context.catalog?.campaigns.find((campaign) => campaign.id === context.selectedId) ?? context.catalog?.campaigns[0] ?? null;
  return `<section class="workspace library-workspace"><div class="campaign-list">${context.catalog?.campaigns.length ? context.catalog.campaigns.map((campaign) => `
    <button class="campaign-card ${campaign.id === selected?.id ? "selected" : ""}" data-library-campaign="${escapeHtml(campaign.id)}"><div class="cover cover-${coverClass(campaign.id)}"><span>${escapeHtml(campaign.title.slice(0, 1).toUpperCase())}</span></div><div class="campaign-copy"><h2>${escapeHtml(campaign.title)}</h2><p>by ${escapeHtml(campaign.author)} · v${escapeHtml(campaign.version)}</p></div>${context.isCurrentCatalogCampaign(campaign) ? '<span class="installed">CURRENT</span>' : ""}</button>`).join("") : '<div class="empty-list">No campaigns in this catalog yet.</div>'}</div>
    <article class="campaign-detail">${selected ? renderLibraryDetail(selected, context) : '<div class="empty-detail"><div class="empty-orb">◇</div><h2>Library is empty.</h2></div>'}</article></section>`;
}
