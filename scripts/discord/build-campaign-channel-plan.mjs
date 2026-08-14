import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";

const GUILD_ID = "967712302767960064";
const INVENTORY_PATH = resolve(process.cwd(), "work/discord-rpc-inventory.json");
const CATALOG_PATH = resolve(process.cwd(), "catalog/catalog.json");
const OUTPUT_PATH = resolve(process.cwd(), "work/discord-campaign-channel-plan.json");
const EXCLUDED_CHANNEL = /(^|-)bug-report(-|$)|patch-notes|(^|-)news(-|$)|troubleshooting|discussion|feedback|^releases-and-news$/i;
const MINIMUM_CANDIDATE_SCORE = 45;

const BRANCH_ALIASES = {
  "Wings of Liberty": ["wol", "wings", "liberty"],
  "Heart of the Swarm": ["hots", "heart", "swarm"],
  "Legacy of the Void": ["lotv", "legacy", "void"],
  "Nova Covert Ops": ["nco", "nova", "covert", "ops"],
};
const STOP_WORDS = new Set(["and", "of", "the", "with", "mod", "campaign", "translated"]);

function normalize(value) {
  return value
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/roguelike/g, "rogue like")
    .replace(/xelnaga/g, "xel naga")
    .replace(/\bai\b/g, "allies")
    .replace(/\bwol\b/g, "wings liberty")
    .replace(/\bhots\b/g, "heart swarm")
    .replace(/\blotv\b/g, "legacy void")
    .replace(/\bnco\b/g, "nova covert ops")
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .trim();
}

function tokens(value) {
  return new Set(normalize(value).split(/\s+/).filter((token) => token.length > 1));
}

function branchTokens(branch) {
  return new Set((BRANCH_ALIASES[branch] ?? []).flatMap((alias) => [...tokens(alias)]));
}

function coreTokens(value, branch) {
  const ignoredTokens = branchTokens(branch);
  return new Set([...tokens(value)].filter((token) => !ignoredTokens.has(token) && !STOP_WORDS.has(token)));
}

function tokenKey(values) {
  return [...values].sort().join(" ");
}

function overlapScore(left, right) {
  const intersection = [...left].filter((token) => right.has(token)).length;
  const union = new Set([...left, ...right]).size;
  return union ? intersection / union : 0;
}

function branchScore(channelTokens, branch) {
  const aliases = new Set(BRANCH_ALIASES[branch] ?? []);
  const matches = [...aliases].filter((alias) => channelTokens.has(alias)).length;
  return Math.min(10, matches * 4);
}

function candidateFor(campaign, channel) {
  const channelNormalized = normalize(channel.name);
  const channelTokens = tokens(channel.name);
  const titleNormalized = normalize(campaign.title);
  const idNormalized = normalize(campaign.id);
  const titleCore = coreTokens(campaign.title, campaign.requirements.campaign);
  const idCore = coreTokens(campaign.id, campaign.requirements.campaign);
  const channelCore = coreTokens(channel.name, campaign.requirements.campaign);
  const titleScore = overlapScore(titleCore, channelCore);
  const idScore = overlapScore(idCore, channelCore);
  let score = Math.round(Math.max(titleScore, idScore) * 82) + branchScore(channelTokens, campaign.requirements.campaign);
  const reasons = [];

  if (channelNormalized === titleNormalized) {
    score = 100;
    reasons.push("normalized title equals channel name");
  } else if (channelNormalized === idNormalized) {
    score = 98;
    reasons.push("normalized catalog ID equals channel name");
  } else if (titleCore.size && tokenKey(titleCore) === tokenKey(channelCore)) {
    score = 96;
    reasons.push("non-branch title tokens equal channel name");
  } else if (idCore.size && tokenKey(idCore) === tokenKey(channelCore)) {
    score = 94;
    reasons.push("non-branch catalog ID tokens equal channel name");
  } else {
    if (titleScore >= 0.5) reasons.push(`title token overlap ${Math.round(titleScore * 100)}%`);
    if (idScore >= 0.5) reasons.push(`ID token overlap ${Math.round(idScore * 100)}%`);
    const branchMatch = branchScore(channelTokens, campaign.requirements.campaign);
    if (branchMatch) reasons.push(`branch cues +${branchMatch}`);
  }

  return {
    channelId: channel.id,
    channelName: channel.name,
    channelUrl: channel.url,
    score,
    reasons,
  };
}

const [catalog, inventory] = await Promise.all([
  readFile(CATALOG_PATH, "utf8").then(JSON.parse),
  readFile(INVENTORY_PATH, "utf8").then(JSON.parse),
]);
const guild = inventory.guilds?.find((item) => item.id === GUILD_ID);
if (!guild) throw new Error(`Guild ${GUILD_ID} was not found in ${INVENTORY_PATH}. Run npm run discord:inventory while logged in first.`);

const eligibleChannels = guild.channels.filter((channel) => channel.type === 0 && !EXCLUDED_CHANNEL.test(channel.name));
const mappings = catalog.campaigns.map((campaign) => {
  const candidates = eligibleChannels
    .map((channel) => candidateFor(campaign, channel))
    .filter((candidate) => candidate.score >= MINIMUM_CANDIDATE_SCORE)
    .sort((left, right) => right.score - left.score || left.channelName.localeCompare(right.channelName))
    .slice(0, 3);
  const recommended = candidates[0] ?? null;
  return {
    campaignId: campaign.id,
    title: campaign.title,
    branch: campaign.requirements.campaign,
    status: recommended?.score >= 90 ? "review-recommended" : recommended ? "review-needed" : "unmatched",
    recommendedChannel: recommended,
    candidates,
  };
});

const output = {
  format: 1,
  generatedAt: new Date().toISOString(),
  source: {
    catalogPath: "catalog/catalog.json",
    inventoryPath: "work/discord-rpc-inventory.json",
    guildId: GUILD_ID,
    eligibleChannelCount: eligibleChannels.length,
  },
  instructions: "All matches are suggestions only. Move approved pairs into the committed mapping before exporting messages.",
  mappings,
};
await mkdir(dirname(OUTPUT_PATH), { recursive: true });
await writeFile(OUTPUT_PATH, `${JSON.stringify(output, null, 2)}\n`, "utf8");

const totals = mappings.reduce((count, mapping) => {
  count[mapping.status] += 1;
  return count;
}, { "review-recommended": 0, "review-needed": 0, unmatched: 0 });
console.log(`Wrote ${OUTPUT_PATH}: ${totals["review-recommended"]} recommended, ${totals["review-needed"]} review-needed, ${totals.unmatched} unmatched.`);
