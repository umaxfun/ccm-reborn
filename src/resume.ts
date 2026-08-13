import type { Campaign, SavedCampaignResume } from "./types";

export function profileName(path: string) {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? "unknown";
}

export function resumeFor(resumes: SavedCampaignResume[], campaignId: string | undefined) {
  return campaignId ? resumes.find((resume) => resume.campaignId === campaignId) ?? null : null;
}

export function sortCampaignsByRecentPlay(campaigns: Campaign[], resumes: SavedCampaignResume[]) {
  return [...campaigns].sort((left, right) => {
    const leftPlayed = resumeFor(resumes, left.id)?.lastPlayedAt ?? null;
    const rightPlayed = resumeFor(resumes, right.id)?.lastPlayedAt ?? null;
    if (leftPlayed !== null || rightPlayed !== null) {
      if (leftPlayed === null) return 1;
      if (rightPlayed === null) return -1;
      if (leftPlayed !== rightPlayed) return rightPlayed - leftPlayed;
    }
    return left.title.localeCompare(right.title, undefined, { sensitivity: "base" }) || left.id.localeCompare(right.id);
  });
}

function missionLabel(map: string | null) {
  if (!map) return "checkpoint detected";
  const filename = map.split(/[\\/]/).at(-1) ?? map;
  return filename.replace(/\.SC2Map$/i, "");
}

export function resumeSummary(resume: SavedCampaignResume | null) {
  if (!resume?.lastPlayedAt) return "Not played through CCM yet";
  const date = new Date(resume.lastPlayedAt * 1000).toLocaleDateString();
  const save = resume.latestSave;
  if (resume.lastPlayedSource === "legacy-ccm-snapshot") {
    return `Played · legacy CCM snapshot ${date}${save ? ` · checkpoint: ${missionLabel(save.map)}` : ""}`;
  }
  return `Last played ${date} · checkpoint: ${missionLabel(save?.map ?? null)}`;
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
  return `Last checkpoint: ${missionLabel(save.map)}. To continue: in SC2 choose Load, then select “${profileName(save.relativePath)}”. Saved ${savedAt} · ${resume.saveCount} save${resume.saveCount === 1 ? "" : "s"} for this campaign.`;
}
