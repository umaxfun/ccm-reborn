import { invoke } from "@tauri-apps/api/core";
import type { Catalog, SavedCampaignResume } from "./types";

const escapeHtml = (value: string) => value.replace(/[&<>'"]/g, (character) =>
  ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character]!,
);

export function renderLegacyMigration(catalog: Catalog | null, resumes: SavedCampaignResume[], profileDir: string, busy: boolean) {
  const snapshots = resumes.filter((resume) => resume.legacyMigrationPending);
  if (!snapshots.length) return "";
  return `<section class="legacy-migration"><p><strong>Legacy CCM saves need an account.</strong> They are read-only until you explicitly attach them to the selected SC2 profile.</p><div>${snapshots.map((resume) => {
    const title = catalog?.campaigns.find((campaign) => campaign.id === resume.campaignId)?.title ?? resume.campaignId;
    return `<button class="ghost compact" data-action="migrate-legacy" data-campaign="${escapeHtml(resume.campaignId)}" ${!profileDir || busy ? "disabled" : ""}>Migrate ${escapeHtml(title)}</button>`;
  }).join("")}</div></section>`;
}

export async function migrateLegacyProfile(campaignId: string, profileDir: string) {
  if (!profileDir) throw new Error("Choose the StarCraft II account that owns these saves first.");
  const confirmed = window.confirm(
    "Copy this legacy CCM save profile to the selected StarCraft II account? The original legacy snapshot will be kept unchanged as a rollback copy.",
  );
  if (!confirmed) return null;
  return invoke<{ campaignId: string; filesCopied: number }>("migrate_legacy_profile", {
    request: { campaignId, profileDir },
  });
}
