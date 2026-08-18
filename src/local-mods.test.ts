import { describe, expect, it } from "vitest";
import { mergeCatalog } from "./local-mods";
import { renderAddLocalDialog } from "./render-add-local-dialog";
import { campaignSlot } from "./domain";
import { sortCampaignsByRecentPlay } from "./resume";
import type { Catalog, LocalModEntry, SavedCampaignResume } from "./types";

const localMod = (id: string, title: string, campaign: string): LocalModEntry => ({
  id,
  title,
  author: "",
  version: "",
  description: "",
  campaign,
  archiveFile: `${id}.zip`,
  archivePath: `/home/player/.ccm-reborn/local/archives/${id}.zip`,
  sha256: "a".repeat(64),
  size: 1024,
  addedAt: 10,
});

const cloudCampaign = (id: string, title: string, campaign: string) => ({
  id,
  title,
  author: "Cloud author",
  version: "1.0",
  description: "From the community catalogue.",
  tags: [],
  requirements: { campaign },
  package: { source: `https://example.invalid/${id}.zip`, sha256: "b".repeat(64), size: 2048 },
});

const cloud: Catalog = {
  name: "Community",
  updatedAt: "2026-08-18T00:00:00Z",
  sourceKind: "remote",
  campaigns: [
    cloudCampaign("zerus-rising", "Zerus Rising", "Heart of the Swarm"),
    cloudCampaign("aiur-reborn", "Aiur Reborn", "Legacy of the Void"),
  ],
};

const playedAt = (campaignId: string, lastPlayedAt: number | null): SavedCampaignResume => ({
  campaignId,
  saveCount: lastPlayedAt ? 1 : 0,
  latestSave: null,
  unverifiedSaveCount: 0,
  lastPlayedAt,
  lastPlayedSource: lastPlayedAt ? "verified-save" : null,
  legacyMigrationPending: false,
});

describe("mergeCatalog", () => {
  it("appends local mods to the cloud list and marks only those as local", () => {
    const merged = mergeCatalog(cloud, [localMod("local-my-hots-mod", "My HotS Mod", "Heart of the Swarm")])!;
    expect(merged.campaigns.map((campaign) => campaign.id)).toEqual([
      "zerus-rising",
      "aiur-reborn",
      "local-my-hots-mod",
    ]);
    expect(merged.campaigns.filter((campaign) => campaign.isLocal).map((campaign) => campaign.id))
      .toEqual(["local-my-hots-mod"]);
    // The stored copy is what an install reads, not the player's own file.
    expect(merged.campaigns.at(-1)!.package.source).toContain(".ccm-reborn/local/archives/");
  });

  it("keeps local mods visible when the cloud catalogue is unavailable", () => {
    const merged = mergeCatalog(null, [localMod("local-solo", "Solo", "Wings of Liberty")])!;
    expect(merged.sourceKind).toBe("local");
    expect(merged.campaigns).toHaveLength(1);
    expect(mergeCatalog(null, [])).toBeNull();
  });

  it("never lists a local mod twice when its id already exists in the cloud", () => {
    const merged = mergeCatalog(cloud, [localMod("zerus-rising", "Zerus Rising", "Heart of the Swarm")])!;
    expect(merged.campaigns).toHaveLength(2);
    expect(merged.campaigns.some((campaign) => campaign.isLocal)).toBe(false);
  });

  it("routes a local mod into the campaign branch its metadata declared", () => {
    const merged = mergeCatalog(cloud, [localMod("local-nova", "Nova Extra", "Nova Covert Ops")])!;
    const local = merged.campaigns.find((campaign) => campaign.id === "local-nova")!;
    expect(campaignSlot(local.requirements.campaign)).toBe("nova-covert-ops");
  });
});

describe("add-local-mod dialog", () => {
  const inspection = {
    title: "My <HotS> Mod",
    author: "Kit",
    version: "1.0.2",
    description: "Short text",
    campaign: "Heart of the Swarm",
    targetPath: "Maps/Campaign/swarm",
    sha256: "c".repeat(64),
    size: 31_947_366,
    files: 42,
    suggestedId: "local-my-hots-mod",
  };

  it("prefills editable fields, escapes titles and states what the hash proves", () => {
    const html = renderAddLocalDialog(inspection, { title: inspection.title, author: "", version: "" }, "/tmp/mod.zip");
    expect(html).toContain('id="local-mod-title"');
    expect(html).toContain('id="local-mod-author"');
    expect(html).toContain('id="local-mod-version"');
    expect(html).toContain("My &lt;HotS&gt; Mod");
    expect(html).not.toContain("<HotS>");
    expect(html).toContain("SHA-256 FROM YOUR FILE");
    expect(html).toContain("Maps/Campaign/swarm");
    // Adding must never read as installing.
    expect(html).toContain("Adding changes nothing in StarCraft II");
  });
});

describe("recent-play ordering across cloud and local mods", () => {
  it("puts a recently played local mod above unplayed cloud mods", () => {
    const merged = mergeCatalog(cloud, [
      localMod("local-played", "Played Local", "Heart of the Swarm"),
      localMod("local-fresh", "Aaa Fresh Local", "Heart of the Swarm"),
    ])!;
    const swarm = merged.campaigns.filter(
      (campaign) => campaignSlot(campaign.requirements.campaign) === "heart-of-the-swarm",
    );
    const ordered = sortCampaignsByRecentPlay(swarm, [
      playedAt("local-played", 1_800_000_000),
      playedAt("zerus-rising", null),
    ]);
    expect(ordered.map((campaign) => campaign.id)).toEqual([
      "local-played",
      // Everything without progress stays alphabetical below it.
      "local-fresh",
      "zerus-rising",
    ]);
  });

  it("orders played cloud and local mods purely by recency", () => {
    const merged = mergeCatalog(cloud, [localMod("local-older", "Older Local", "Heart of the Swarm")])!;
    const swarm = merged.campaigns.filter(
      (campaign) => campaignSlot(campaign.requirements.campaign) === "heart-of-the-swarm",
    );
    const ordered = sortCampaignsByRecentPlay(swarm, [
      playedAt("local-older", 1_700_000_000),
      playedAt("zerus-rising", 1_800_000_000),
    ]);
    expect(ordered.map((campaign) => campaign.id)).toEqual(["zerus-rising", "local-older"]);
  });

  it("keeps an unplayed local mod above the alphabetical cloud list", () => {
    // "Zulu" sorts last alphabetically, but the player added it deliberately,
    // so it must not be buried below the cloud entries.
    const merged = mergeCatalog(cloud, [localMod("local-zulu", "Zulu Mod", "Heart of the Swarm")])!;
    const swarm = merged.campaigns.filter(
      (campaign) => campaignSlot(campaign.requirements.campaign) === "heart-of-the-swarm",
    );
    expect(sortCampaignsByRecentPlay(swarm, []).map((campaign) => campaign.id))
      .toEqual(["local-zulu", "zerus-rising"]);
  });

  it("puts the most recently added local mod first among unplayed local mods", () => {
    const merged = mergeCatalog(cloud, [
      { ...localMod("local-old", "Aaa Older", "Heart of the Swarm"), addedAt: 100 },
      { ...localMod("local-new", "Zzz Newer", "Heart of the Swarm"), addedAt: 200 },
    ])!;
    const swarm = merged.campaigns.filter(
      (campaign) => campaignSlot(campaign.requirements.campaign) === "heart-of-the-swarm",
    );
    expect(sortCampaignsByRecentPlay(swarm, []).map((campaign) => campaign.id))
      .toEqual(["local-new", "local-old", "zerus-rising"]);
  });
});
