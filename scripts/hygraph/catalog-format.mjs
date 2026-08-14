const BRANCHES = new Map([
  ["Wings of Liberty", "WINGS_OF_LIBERTY"],
  ["Heart of the Swarm", "HEART_OF_THE_SWARM"],
  ["Legacy of the Void", "LEGACY_OF_THE_VOID"],
  ["Nova Covert Ops", "NOVA_COVERT_OPS"],
]);

const BRANCH_ORDER = new Map([...BRANCHES.keys()].map((branch, index) => [branch, index]));
const SHA256 = /^[a-f0-9]{64}$/;
const CAMPAIGN_ID = /^[A-Za-z0-9_-]{1,80}$/;

export const PRODUCTION_CATALOG_NAME = "CCM Reborn · Community campaigns";

export function branchApiId(branch) {
  const apiId = BRANCHES.get(branch);
  if (!apiId) throw new Error(`Unsupported campaign branch: ${branch}`);
  return apiId;
}

export function branchDisplayName(apiId) {
  for (const [displayName, candidateApiId] of BRANCHES) {
    if (candidateApiId === apiId) return displayName;
  }
  throw new Error(`Unsupported CampaignBranch value from Hygraph: ${apiId}`);
}

export function catalogCampaignToCmsData(campaign, catalogOrder) {
  return {
    campaignId: campaign.id,
    catalogOrder,
    title: campaign.title,
    author: campaign.author,
    shortDescription: campaign.description,
    tags: campaign.tags,
    branch: branchApiId(campaign.requirements.campaign),
  };
}

export function cmsCampaignToCatalogCampaign(campaign) {
  if (!campaign || typeof campaign !== "object") throw new Error("Hygraph returned an invalid campaign.");
  if (!campaign.currentRelease) {
    throw new Error(`Campaign ${campaign.campaignId ?? "<unknown>"} has no published release.`);
  }

  return {
    catalogOrder: campaign.catalogOrder,
    id: campaign.campaignId,
    title: campaign.title,
    author: campaign.author,
    version: campaign.currentRelease.version,
    description: campaign.shortDescription,
    tags: campaign.tags,
    requirements: { campaign: branchDisplayName(campaign.branch) },
    package: {
      url: campaign.currentRelease.packageUrl,
      sha256: campaign.currentRelease.packageSha256,
      size: campaign.currentRelease.packageSize,
    },
  };
}

export function sortCampaigns(campaigns) {
  return [...campaigns].sort((left, right) => (
    (BRANCH_ORDER.get(left.requirements.campaign) ?? Number.MAX_SAFE_INTEGER)
      - (BRANCH_ORDER.get(right.requirements.campaign) ?? Number.MAX_SAFE_INTEGER)
      || left.title.localeCompare(right.title, "en")
      || left.id.localeCompare(right.id, "en")
  ));
}

export function validateCatalog(catalog) {
  if (!catalog || catalog.format !== 1 || !Array.isArray(catalog.campaigns)) {
    throw new Error("Catalog must be format 1 and contain a campaigns array.");
  }
  if (typeof catalog.name !== "string" || catalog.name.trim() === "") {
    throw new Error("Catalog name is required.");
  }
  if (typeof catalog.updatedAt !== "string" || Number.isNaN(Date.parse(catalog.updatedAt))) {
    throw new Error("Catalog updatedAt must be an ISO timestamp.");
  }

  const ids = new Set();
  const packageUrls = new Set();
  for (const campaign of catalog.campaigns) {
    const prefix = `Campaign ${campaign?.id ?? "<unknown>"}`;
    if (!CAMPAIGN_ID.test(campaign?.id ?? "")) throw new Error(`${prefix} has an invalid id.`);
    if (ids.has(campaign.id)) throw new Error(`Campaign id ${campaign.id} occurs more than once.`);
    ids.add(campaign.id);
    for (const field of ["title", "author", "version", "description"]) {
      if (typeof campaign[field] !== "string" || campaign[field].trim() === "") {
        throw new Error(`${prefix} is missing ${field}.`);
      }
    }
    if (!Array.isArray(campaign.tags) || campaign.tags.some((tag) => typeof tag !== "string" || tag.trim() === "")) {
      throw new Error(`${prefix} has invalid tags.`);
    }
    if (!BRANCHES.has(campaign?.requirements?.campaign)) throw new Error(`${prefix} has an unsupported branch.`);
    if (typeof campaign?.package?.url !== "string" || !campaign.package.url.startsWith("https://")) {
      throw new Error(`${prefix} needs an HTTPS package URL.`);
    }
    if (packageUrls.has(campaign.package.url)) throw new Error(`${prefix} shares a package URL with another campaign.`);
    packageUrls.add(campaign.package.url);
    if (!SHA256.test(campaign.package.sha256 ?? "")) throw new Error(`${prefix} has an invalid SHA-256.`);
    if (!Number.isSafeInteger(campaign.package.size) || campaign.package.size <= 0) {
      throw new Error(`${prefix} has an invalid package size.`);
    }
  }
}

export function compareCatalogs(baseline, candidate, allowedRemovals = new Set()) {
  validateCatalog(baseline);
  validateCatalog(candidate);

  const nextById = new Map(candidate.campaigns.map((campaign) => [campaign.id, campaign]));
  const removed = baseline.campaigns.filter((campaign) => !nextById.has(campaign.id)).map((campaign) => campaign.id);
  const unexpectedRemovals = removed.filter((id) => !allowedRemovals.has(id));
  if (unexpectedRemovals.length) {
    throw new Error(`Refusing to remove campaigns without --allow-remove: ${unexpectedRemovals.join(", ")}`);
  }

  for (const previous of baseline.campaigns) {
    const next = nextById.get(previous.id);
    if (!next) continue;
    if (previous.package.url === next.package.url
      && (previous.package.sha256 !== next.package.sha256 || previous.package.size !== next.package.size)) {
      throw new Error(`Campaign ${previous.id} changes immutable package metadata without changing package.url.`);
    }
    if (previous.package.url !== next.package.url
      && previous.version === next.version
      && previous.package.sha256 === next.package.sha256
      && previous.package.size === next.package.size) {
      throw new Error(`Campaign ${previous.id} changes package.url without a release change.`);
    }
  }

  return { removed, added: candidate.campaigns.filter((campaign) => !baseline.campaigns.some((item) => item.id === campaign.id)).map((campaign) => campaign.id) };
}

export function sameCatalogContent(left, right) {
  const withoutTimestamp = ({ updatedAt: _updatedAt, ...catalog }) => catalog;
  return JSON.stringify(withoutTimestamp(left)) === JSON.stringify(withoutTimestamp(right));
}

export function makeProductionCatalog(cmsCampaigns, existingCatalog) {
  const orderedCampaigns = cmsCampaigns
    .map(cmsCampaignToCatalogCampaign)
    .sort((left, right) => {
      if (Number.isSafeInteger(left.catalogOrder) && Number.isSafeInteger(right.catalogOrder)) {
        return left.catalogOrder - right.catalogOrder || left.id.localeCompare(right.id, "en");
      }
      return sortCampaigns([left, right])[0] === left ? -1 : 1;
    })
    .map(({ catalogOrder: _catalogOrder, ...campaign }) => campaign);
  const candidate = {
    format: 1,
    name: PRODUCTION_CATALOG_NAME,
    updatedAt: "1970-01-01T00:00:00.000Z",
    campaigns: orderedCampaigns,
  };
  if (existingCatalog && sameCatalogContent(existingCatalog, candidate)) {
    candidate.updatedAt = existingCatalog.updatedAt;
  } else {
    candidate.updatedAt = new Date().toISOString();
  }
  validateCatalog(candidate);
  return candidate;
}
