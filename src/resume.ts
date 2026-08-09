import type { SavedCampaignResume } from "./types";

export function profileName(path: string) {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? "unknown";
}

export function resumeFor(resumes: SavedCampaignResume[], campaignId: string | undefined) {
  return campaignId ? resumes.find((resume) => resume.campaignId === campaignId) ?? null : null;
}

export function resumeInstruction(resume: SavedCampaignResume | null) {
  if (!resume?.latestSave) {
    const leftovers = resume?.unverifiedSaveCount
      ? ` CCM kept ${resume.unverifiedSaveCount} slot save${resume.unverifiedSaveCount === 1 ? "" : "s"} for rollback, but none are verified as belonging to this mod.`
      : "";
    return `Start new campaign — no verified save exists for this mod.${leftovers}`;
  }
  const save = resume.latestSave;
  const savedAt = new Date(save.modifiedAt * 1000).toLocaleString();
  return `To continue: in SC2 choose Load, then select “${profileName(save.relativePath)}”. Latest checkpoint saved ${savedAt} · ${resume.saveCount} save${resume.saveCount === 1 ? "" : "s"} for this campaign.`;
}
