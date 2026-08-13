import type { Campaign, SavedCampaignResume } from "./types";

// Community campaigns commonly reuse StarCraft II's canonical map IDs. The
// save records that ID, not its player-facing title. Unknown maps deliberately
// fall back to their internal name: a mod may reuse an original slot for a new
// map.
const missionTitles: Record<string, string> = {
  // Wings of Liberty
  traynor01: "Liberation Day",
  traynor02: "The Outlaws",
  traynor03: "Zero Hour",
  thanson01: "The Evacuation",
  thanson02: "Outbreak",
  thanson03: "Safe Haven",
  thanson04: "Haven's Fall",
  thorner01: "The Great Train Robbery",
  thorner02: "Cutthroat",
  thorner03: "Engine of Destruction",
  thorner04: "Media Blitz",
  thorner05: "Piercing the Shroud",
  thorner05s: "Piercing the Shroud",
  ttosh01: "The Devil's Playground",
  ttosh02: "Welcome to the Jungle",
  ttosh03a: "Breakout",
  ttosh03b: "Ghost of a Chance",
  ttychus01: "Smash and Grab",
  ttychus02: "The Dig",
  ttychus03: "The Moebius Factor",
  ttychus04: "Supernova",
  ttychus05: "Maw of the Void",
  tvalerian01: "The Gates of Hell",
  tvalerian02a: "Belly of the Beast",
  tvalerian02b: "Shatter the Sky",
  tvalerian03: "All In",
  tzeratul01: "Whispers of Doom",
  tzeratul02: "A Sinister Turn",
  tzeratul03: "Echoes of the Future",
  tzeratul04: "In Utter Darkness",

  // Heart of the Swarm
  zchar01: "Domination",
  zchar02: "Fire in the Sky",
  zchar03: "Old Soldiers",
  zexpedition01: "Harvest of Screams",
  zexpedition02: "Shoot the Messenger",
  zexpedition03: "Enemy Within",
  zhybrid01: "Infested",
  zhybrid02: "Hand of Darkness",
  zhybrid03: "Phantoms of the Void",
  zkorhal01: "Planetfall",
  zkorhal02: "Death From Above",
  zkorhal03: "The Reckoning",
  zlab01: "Lab Rat",
  zlab02: "Back in the Saddle",
  zlab03: "Rendezvous",
  zspace01: "With Friends Like These…",
  zspace02: "Conviction",
  zstoryevolve: "Evolution Pit",
  zzerus01: "Waking the Ancient",
  zzerus02: "The Crucible",
  zzerus03: "Supreme",

  // Legacy of the Void, including its prologue and epilogue
  paiur01: "For Aiur!",
  paiur02: "The Growing Shadow",
  paiur03: "Spear of Adun",
  paiur04: "Templar's Return",
  paiur05: "The Host",
  paiur06: "Salvation",
  pkorhal01: "Sky Shield",
  pkorhal02: "Brothers in Arms",
  pmoebius01: "Templar's Charge",
  ppurifier01: "Forbidden Weapon",
  ppurifier02: "Unsealing the Past",
  ppurifier03: "Purification",
  pshakuras01: "Amon's Reach",
  pshakuras02: "Last Stand",
  ptaldarim01: "Steps of the Rite",
  ptaldarim02: "Rak'Shir",
  pulnar01: "Temple of Unification",
  pulnar02: "The Infinite Cycle",
  pulnar03: "Harbinger of Oblivion",
  sc2epilogue01: "Into the Void",
  sc2epilogue02: "The Essence of Eternity",
  sc2epilogue03: "Amon's Fall",
  pepilogue01: "Into the Void",
  pepilogue02: "The Essence of Eternity",
  pepilogue03: "Amon's Fall",
  voidprologue01: "Dark Whispers",
  voidprologue02: "Ghosts in the Fog",
  voidprologue03: "Evil Awoken",

  // Nova Covert Ops
  nova01: "The Escape",
  nova02: "Sudden Strike",
  nova03: "Enemy Intelligence",
  nova04: "Trouble in Paradise",
  nova05: "Night Terrors",
  nova06: "Flashpoint",
  nova07: "In the Enemy's Shadow",
  nova08: "Dark Skies",
  nova09: "End Game",
};

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
  const mapId = filename.replace(/\.SC2Map$/i, "");
  return missionTitles[mapId.toLowerCase()] ?? mapId;
}

function saveLabel(path: string) {
  return profileName(path).replace(/\.SC2Save$/i, "");
}

export function resumeCheckpoint(resume: SavedCampaignResume | null) {
  return resume?.latestSave ? missionLabel(resume.latestSave.map) : null;
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
    return "Start a new campaign — there is no verified save for this mod yet.";
  }
  const save = resume.latestSave;
  return `Load “${saveLabel(save.relativePath)}” in StarCraft II.`;
}
