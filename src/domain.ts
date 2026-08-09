import type { Campaign, Inspection } from "./types";

export function campaignSlot(campaign: string) {
  const value = campaign.toLowerCase();
  if (value.includes("wings") || value.includes("liberty") || value.includes("wol")) return "wings-of-liberty";
  if (value.includes("heart") || value.includes("swarm") || value.includes("hots")) return "heart-of-the-swarm";
  if (value.includes("legacy") || value.includes("void") || value.includes("lotv")) return "legacy-of-the-void";
  return "nova-covert-ops";
}

export function coverClass(id: string) {
  return ["ember", "void", "arc", "jade"][Array.from(id).reduce((total, character) => total + character.charCodeAt(0), 0) % 4];
}

export const formatBytes = (bytes: number) => bytes < 1024 * 1024
  ? `${Math.max(1, Math.round(bytes / 1024))} KB`
  : `${(bytes / 1024 / 1024).toFixed(bytes > 1024 * 1024 * 1024 ? 1 : 0)} MB`;

export function isCurrentCatalogCampaign(campaign: Campaign, inspection: Inspection | null) {
  const current = inspection?.activeCampaigns.find((item) => item.slot === campaignSlot(campaign.requirements.campaign));
  return Boolean(current && current.title.trim().toLocaleLowerCase() === campaign.title.trim().toLocaleLowerCase() && current.version.trim() === campaign.version.trim());
}
