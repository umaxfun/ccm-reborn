export type Campaign = {
  id: string;
  title: string;
  author: string;
  version: string;
  description: string;
  tags: string[];
  requirements: { campaign: string };
  package: { source: string; sha256: string; size: number };
};

export type Catalog = {
  name: string;
  updatedAt: string;
  sourceKind: "local" | "remote" | "cached";
  campaigns: Campaign[];
};

export type CurrentCampaign = {
  slot: string;
  campaign: string;
  title: string;
  author: string;
  version: string;
  isModified: boolean;
};

export type Inspection = {
  exists: boolean;
  path: string;
  activeCampaign: { id: string; title: string; slot: string; targetPath: string; files: number } | null;
  managedCampaigns: { id: string; title: string; slot: string; targetPath: string; files: number }[];
  activeCampaigns: CurrentCampaign[];
  recoveryPerformed: boolean;
};

export type GameDirectoryCandidate = { path: string; label: string };
export type StarcraftProfileCandidate = { path: string; label: string };
export type RestoreResult = { restoredFiles: number; conflicts: string[] };
export type SavedCampaignSave = {
  relativePath: string;
  modifiedAt: number;
  map: string | null;
  detailsAvailable: boolean;
};
export type SavedCampaignResume = {
  campaignId: string;
  saveCount: number;
  latestSave: SavedCampaignSave | null;
  unverifiedSaveCount: number;
  lastPlayedAt: number | null;
  lastPlayedSource: "verified-save" | "legacy-ccm-snapshot" | null;
  legacyMigrationPending: boolean;
};
export type InstallResult = {
  campaignId: string;
  title: string;
  version: string;
  manifestPath: string;
  packageSha256: string;
  filesInstalled: number;
};
export type InstallProgress = {
  operationId: string;
  phase: string;
  message: string;
  downloadedBytes: number | null;
  totalBytes: number | null;
  completedFiles: number | null;
  totalFiles: number | null;
};
export type ProgressFilePlan = {
  relativePath: string;
  source: string;
  destination: string;
  kind: string;
  action: string;
  size: number;
  sha256: string;
  detail: string | null;
};
export type FileChangePlan = {
  source: string;
  destination: string;
  operation: string;
  kind: string;
  size: number;
  sha256: string | null;
  detail: string | null;
};
export type ProgressKeyChange = {
  key: string;
  currentValue: string;
  plannedValue: string;
  action: string;
};
export type BankPlan = {
  relativePath: string;
  source: string;
  destination: string;
  sections: number;
  keys: number;
  keysChangedInPlace: number;
  note: string;
};
export type DryRunPlan = {
  operationId: string;
  campaignId: string;
  title: string;
  gameDirectory: string;
  targetPath: string;
  archiveSize: number;
  archiveSha256: string;
  updateKind: string;
  previousInstallManifest: string | null;
  previousInstallCampaignId: string | null;
  previousInstallVersion: string | null;
  previousInstallSha256: string | null;
  previousInstallFiles: number;
  packageFiles: number;
  packageBytes: number;
  campaignFilesToClear: number;
  campaignBytesToClear: number;
  dependencyRoots: string[];
  dependencyFilesToReplace: number;
  filesToBackup: number;
  profilePath: string | null;
  profileStorePath: string | null;
  profileFilesToSnapshot: number;
  profileBytesToSnapshot: number;
  profileFilesToRestore: number;
  profileBytesToRestore: number;
  progressUpdates: number;
  progressFiles: ProgressFilePlan[];
  progressKeys: ProgressKeyChange[];
  bankPlans: BankPlan[];
  fileChanges: FileChangePlan[];
  warnings: string[];
};
