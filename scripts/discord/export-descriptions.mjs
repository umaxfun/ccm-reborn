import { access, chmod, mkdir, readFile, writeFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import net from "node:net";
import { dirname, join, resolve } from "node:path";
import { discordThreadUrl, exportThread } from "./export-format.mjs";

const IPC_HANDSHAKE = 0;
const IPC_FRAME = 1;
const FORUM_CHANNEL_TYPE = 15;
const THREAD_CHANNEL_TYPES = new Set([10, 11, 12]);
const DEFAULT_REDIRECT_URI = "http://localhost:3344/discord-rpc";
const OUTPUT_PATH = resolve(process.cwd(), "work/discord-descriptions.json");
const INVENTORY_PATH = resolve(process.cwd(), "work/discord-rpc-inventory.json");
const SOURCE_MAP_PATH = resolve(process.cwd(), "scripts/discord/campaign-source-map.json");
const OAUTH_CACHE_PATH = resolve(process.cwd(), "work/discord-rpc-oauth.json");
const COMMAND_TIMEOUT_MS = 30_000;
const REQUIRED_SCOPES = ["rpc", "messages.read", "guilds"];

function requireEnvironment(name) {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`${name} is required. Add it to .env before running this command.`);
  return value;
}

function hasFlag(flag) {
  return process.argv.slice(2).includes(flag);
}

function flagValues(flag) {
  const values = [];
  for (let index = 2; index < process.argv.length; index += 1) {
    if (process.argv[index] !== flag) continue;
    const value = process.argv[index + 1];
    if (!value || value.startsWith("--")) throw new Error(`${flag} requires a value.`);
    values.push(value);
    index += 1;
  }
  return values;
}

function commaSeparated(values) {
  return values
    .flatMap((value) => value.split(","))
    .map((value) => value.trim())
    .filter(Boolean);
}

function uniqueStrings(values) {
  return [...new Set(values)];
}

function usage() {
  console.log(`Usage:
  npm run discord:inventory
  npm run discord:export -- --channel <channel-id> [--channel <channel-id> ...]
  npm run discord:export -- --from-inventory
  npm run discord:export -- --from-source-map

Required .env values:
  DISCORD_CLIENT_ID, DISCORD_CLIENT_SECRET, DISCORD_RPC_REDIRECT_URI

Optional .env values:
  DISCORD_GUILD_ID              Limits inventory to one server.
  DISCORD_RPC_FORUM_CHANNEL_ID  Limits discovered thread candidates to one forum.
  DISCORD_RPC_CHANNEL_IDS       Comma-separated channel IDs for export.

\`--thread\` and DISCORD_RPC_THREAD_IDS remain accepted for forum-post exports.
\`--from-source-map\` reads the reviewed campaign source map and retains its
campaign-to-source mapping in the export.

The script uses local Discord RPC only. Start Discord Desktop and approve the
OAuth prompt; do not paste a Discord user token into .env.`);
}

function rpcError(payload) {
  const details = payload?.data ?? {};
  const code = details.code ? ` (${details.code})` : "";
  return new Error(`Discord RPC ${payload?.cmd ?? "request"} failed${code}: ${details.message ?? "unknown error"}`);
}

class DiscordRpcClient {
  constructor(socket) {
    this.socket = socket;
    this.buffer = Buffer.alloc(0);
    this.pending = new Map();
    this.ready = new Promise((resolveReady, rejectReady) => {
      this.resolveReady = resolveReady;
      this.rejectReady = rejectReady;
    });
    // The Discord client can close immediately after connect (for example when
    // it is logged out), before handshake() attaches its await handler.
    this.ready.catch(() => {});

    socket.on("data", (chunk) => this.receive(chunk));
    socket.once("error", (error) => this.fail(error));
    socket.once("close", () => this.fail(new Error("Discord Desktop closed the local RPC connection.")));
  }

  send(opcode, payload) {
    const body = Buffer.from(JSON.stringify(payload), "utf8");
    const header = Buffer.alloc(8);
    header.writeInt32LE(opcode, 0);
    header.writeInt32LE(body.length, 4);
    this.socket.write(Buffer.concat([header, body]));
  }

  receive(chunk) {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    while (this.buffer.length >= 8) {
      const opcode = this.buffer.readInt32LE(0);
      const length = this.buffer.readInt32LE(4);
      if (length < 0 || length > 10_000_000) {
        this.fail(new Error(`Discord RPC sent an invalid frame length: ${length}.`));
        return;
      }
      if (this.buffer.length < 8 + length) return;
      const frame = this.buffer.subarray(8, 8 + length);
      this.buffer = this.buffer.subarray(8 + length);
      let payload;
      try {
        payload = JSON.parse(frame.toString("utf8"));
      } catch (error) {
        this.fail(new Error(`Could not parse a Discord RPC response: ${error.message}`));
        return;
      }
      this.handleFrame(opcode, payload);
    }
  }

  handleFrame(opcode, payload) {
    if (opcode === 2) {
      const code = payload?.code ? ` (${payload.code})` : "";
      this.fail(new Error(`Discord RPC closed${code}: ${payload?.message ?? "unknown reason"}`));
      return;
    }
    if (payload?.evt === "READY") this.resolveReady(payload.data);
    const pending = payload?.nonce ? this.pending.get(payload.nonce) : undefined;
    if (!pending) return;
    this.pending.delete(payload.nonce);
    clearTimeout(pending.timeout);
    if (payload.evt === "ERROR") pending.reject(rpcError(payload));
    else pending.resolve(payload.data);
  }

  fail(error) {
    this.rejectReady(error);
    for (const { reject, timeout } of this.pending.values()) {
      clearTimeout(timeout);
      reject(error);
    }
    this.pending.clear();
  }

  async handshake(clientId) {
    this.send(IPC_HANDSHAKE, { v: 1, client_id: clientId });
    await this.ready;
  }

  command(cmd, args = {}) {
    const nonce = randomUUID();
    return new Promise((resolveCommand, rejectCommand) => {
      const timeout = setTimeout(() => {
        this.pending.delete(nonce);
        rejectCommand(new Error(`Timed out waiting for Discord RPC ${cmd}.`));
      }, COMMAND_TIMEOUT_MS);
      this.pending.set(nonce, { resolve: resolveCommand, reject: rejectCommand, timeout });
      this.send(IPC_FRAME, { cmd, nonce, args });
    });
  }

  close() {
    this.socket.end();
    this.socket.destroy();
  }
}

function candidateIpcPaths() {
  const directories = uniqueStrings([
    process.env.XDG_RUNTIME_DIR,
    process.env.TMPDIR,
    process.env.TMP,
    process.env.TEMP,
    "/tmp",
  ].filter(Boolean));
  return directories.flatMap((directory) => Array.from(
    { length: 10 },
    (_unused, index) => join(directory, `discord-ipc-${index}`),
  ));
}

async function connect(path) {
  return new Promise((resolveConnection, rejectConnection) => {
    const socket = net.createConnection({ path });
    const onError = (error) => {
      socket.destroy();
      rejectConnection(error);
    };
    socket.once("error", onError);
    socket.once("connect", () => {
      socket.off("error", onError);
      resolveConnection(socket);
    });
  });
}

async function openDiscordRpc() {
  const attemptedPaths = [];
  for (const path of candidateIpcPaths()) {
    try {
      await access(path);
    } catch {
      continue;
    }
    attemptedPaths.push(path);
    try {
      return new DiscordRpcClient(await connect(path));
    } catch {
      // A stale socket is harmless; keep looking through Discord's documented range.
    }
  }
  const suffix = attemptedPaths.length ? ` Checked: ${attemptedPaths.join(", ")}.` : "";
  throw new Error(`Could not find a usable local Discord IPC socket. Start Discord Desktop, log in, and retry.${suffix}`);
}

async function discordOAuthTokenRequest(body, description) {
  const response = await fetch("https://discord.com/api/oauth2/token", {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body,
  });
  const result = await response.json().catch(() => null);
  if (!response.ok || !result?.access_token) {
    throw new Error(`Discord OAuth ${description} failed: ${result?.error_description ?? result?.error ?? `HTTP ${response.status}`}`);
  }
  return result;
}

async function exchangeAuthorizationCode({ clientId, clientSecret, redirectUri, code }) {
  return discordOAuthTokenRequest(new URLSearchParams({
    client_id: clientId,
    client_secret: clientSecret,
    grant_type: "authorization_code",
    code,
    redirect_uri: redirectUri,
  }), "token exchange");
}

async function refreshAccessToken({ clientId, clientSecret, redirectUri, refreshToken }) {
  return discordOAuthTokenRequest(new URLSearchParams({
    client_id: clientId,
    client_secret: clientSecret,
    grant_type: "refresh_token",
    refresh_token: refreshToken,
    redirect_uri: redirectUri,
  }), "token refresh");
}

async function readOAuthCache() {
  try {
    const cache = JSON.parse(await readFile(OAUTH_CACHE_PATH, "utf8"));
    if (cache.format !== 1 || typeof cache.clientId !== "string" || typeof cache.accessToken !== "string") {
      throw new Error("unexpected format");
    }
    return cache;
  } catch (error) {
    if (error.code === "ENOENT") return null;
    throw new Error(`Could not read ${OAUTH_CACHE_PATH}: ${error.message}. Remove that cache file and authorize again.`);
  }
}

async function writeOAuthCache(clientId, token) {
  const expiresInSeconds = Number(token.expires_in);
  const expiresAt = Number.isFinite(expiresInSeconds) && expiresInSeconds > 0
    ? new Date(Date.now() + expiresInSeconds * 1_000).toISOString()
    : null;
  const cache = {
    format: 1,
    clientId,
    accessToken: token.access_token,
    refreshToken: token.refresh_token ?? null,
    expiresAt,
    updatedAt: new Date().toISOString(),
  };
  await mkdir(dirname(OAUTH_CACHE_PATH), { recursive: true });
  await writeFile(OAUTH_CACHE_PATH, `${JSON.stringify(cache, null, 2)}\n`, { encoding: "utf8", mode: 0o600 });
  await chmod(OAUTH_CACHE_PATH, 0o600);
}

function readOAuthConfiguration() {
  return {
    clientId: requireEnvironment("DISCORD_CLIENT_ID"),
    clientSecret: requireEnvironment("DISCORD_CLIENT_SECRET"),
    redirectUri: process.env.DISCORD_RPC_REDIRECT_URI?.trim() || DEFAULT_REDIRECT_URI,
  };
}

async function authenticateAccessToken(rpc, accessToken) {
  const identity = await rpc.command("AUTHENTICATE", { access_token: accessToken });
  const grantedScopes = new Set(identity?.scopes ?? []);
  for (const scope of REQUIRED_SCOPES) {
    if (!grantedScopes.has(scope)) throw new Error(`Discord did not grant the required ${scope} scope.`);
  }
  return identity;
}

async function authorize(rpc, { clientId, clientSecret, redirectUri }) {
  await rpc.handshake(clientId);
  const cache = await readOAuthCache();
  const cacheMatchesApplication = cache?.clientId === clientId;
  const cacheStillValid = cacheMatchesApplication
    && cache.expiresAt
    && Date.parse(cache.expiresAt) > Date.now() + 60_000;
  if (cacheStillValid) {
    try {
      const identity = await authenticateAccessToken(rpc, cache.accessToken);
      console.log("Reused cached Discord OAuth access token.");
      return identity;
    } catch {
      // Discord may revoke an access token early. Refresh it before prompting again.
    }
  }
  if (cacheMatchesApplication && cache?.refreshToken) {
    try {
      const refreshedToken = await refreshAccessToken({ clientId, clientSecret, redirectUri, refreshToken: cache.refreshToken });
      await writeOAuthCache(clientId, refreshedToken);
      const identity = await authenticateAccessToken(rpc, refreshedToken.access_token);
      console.log("Refreshed cached Discord OAuth access token.");
      return identity;
    } catch {
      // A rejected refresh requires an explicit authorization prompt below.
    }
  }

  const { code } = await rpc.command("AUTHORIZE", {
    client_id: clientId,
    scopes: REQUIRED_SCOPES,
  });
  if (!code) throw new Error("Discord RPC authorization did not return an authorization code.");
  const token = await exchangeAuthorizationCode({ clientId, clientSecret, redirectUri, code });
  await writeOAuthCache(clientId, token);
  return authenticateAccessToken(rpc, token.access_token);
}

async function writeJson(path, value) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

async function readInventoryThreads() {
  const inventory = JSON.parse(await readFile(INVENTORY_PATH, "utf8"));
  if (inventory.format !== 1 || !Array.isArray(inventory?.threads)) {
    throw new Error(`${INVENTORY_PATH} is not a Discord RPC inventory generated by this script.`);
  }
  return inventory.threads.map((thread) => thread.id).filter(Boolean);
}

async function readSourceMapMappings() {
  let sourceMap;
  try {
    sourceMap = JSON.parse(await readFile(SOURCE_MAP_PATH, "utf8"));
  } catch (error) {
    throw new Error(`Could not read ${SOURCE_MAP_PATH}: ${error.message}`);
  }
  if (sourceMap.format !== 1 || !Array.isArray(sourceMap.mappings)) {
    throw new Error(`${SOURCE_MAP_PATH} must contain a format-1 mappings array.`);
  }
  for (const mapping of sourceMap.mappings) {
    if (typeof mapping?.campaignId !== "string" || typeof mapping?.channelId !== "string") {
      throw new Error(`${SOURCE_MAP_PATH} contains a mapping without campaignId or channelId.`);
    }
  }
  return sourceMap.mappings;
}

async function createInventory(rpc, identity) {
  const configuredGuildId = process.env.DISCORD_GUILD_ID?.trim();
  const configuredForumChannelId = process.env.DISCORD_RPC_FORUM_CHANNEL_ID?.trim();
  const guilds = (await rpc.command("GET_GUILDS")).guilds ?? [];
  const selectedGuilds = configuredGuildId
    ? guilds.filter((guild) => guild.id === configuredGuildId)
    : guilds;
  if (configuredGuildId && selectedGuilds.length === 0) {
    throw new Error(`The authorized Discord account cannot see DISCORD_GUILD_ID=${configuredGuildId}.`);
  }

  const guildInventory = [];
  for (const guild of selectedGuilds) {
    const channels = (await rpc.command("GET_CHANNELS", { guild_id: guild.id })).channels ?? [];
    guildInventory.push({
      id: guild.id,
      name: guild.name,
      channels: channels.map((channel) => ({
        id: channel.id,
        name: channel.name,
        type: channel.type,
        parentId: channel.parent_id ?? undefined,
        url: discordThreadUrl(guild.id, channel.id),
      })),
    });
  }

  const allThreadCandidates = guildInventory.flatMap((guild) => guild.channels
    .filter((channel) => THREAD_CHANNEL_TYPES.has(channel.type))
    .map((channel) => ({ ...channel, guildId: guild.id, guildName: guild.name })));
  const threads = configuredForumChannelId
    ? allThreadCandidates.filter((thread) => thread.parentId === configuredForumChannelId)
    : allThreadCandidates;
  if (configuredForumChannelId && threads.length === 0) {
    console.warn(`Discord RPC did not expose any thread candidates under forum ${configuredForumChannelId}.`);
    console.warn("The local RPC API may expose only parent channels or currently loaded threads; review the inventory before exporting.");
  }

  const inventory = {
    format: 1,
    exportedAt: new Date().toISOString(),
    source: {
      type: "discord-local-rpc",
      accountId: identity.user?.id,
      guildId: configuredGuildId ?? undefined,
      forumChannelId: configuredForumChannelId ?? undefined,
    },
    guilds: guildInventory,
    threads,
  };
  await writeJson(INVENTORY_PATH, inventory);
  console.log(`Wrote ${inventory.guilds.length} guild inventories and ${inventory.threads.length} thread candidates to ${INVENTORY_PATH}.`);
}

async function exportDescriptions(rpc, identity) {
  const sourceMapMappings = hasFlag("--from-source-map") ? await readSourceMapMappings() : [];
  const suppliedChannelIds = commaSeparated([
    ...flagValues("--channel"),
    ...flagValues("--thread"),
    process.env.DISCORD_RPC_CHANNEL_IDS ?? "",
    process.env.DISCORD_RPC_THREAD_IDS ?? "",
  ]);
  const threadIds = uniqueStrings([
    ...suppliedChannelIds,
    ...(hasFlag("--from-inventory") ? await readInventoryThreads() : []),
    ...sourceMapMappings.map((mapping) => mapping.channelId),
  ]);
  if (threadIds.length === 0) {
    throw new Error("Provide --channel <channel-id>, DISCORD_RPC_CHANNEL_IDS, --from-inventory, or --from-source-map.");
  }

  const threads = [];
  for (const threadId of threadIds) {
    const channel = await rpc.command("GET_CHANNEL", { channel_id: threadId });
    if (!channel?.id) throw new Error(`Discord RPC returned no channel for thread ${threadId}.`);
    if (channel.type === FORUM_CHANNEL_TYPE) {
      throw new Error(
        `${threadId} is a forum parent channel, not a post thread. Local Discord RPC does not enumerate forum posts; pass a post URL's channel ID instead.`,
      );
    }
    threads.push(exportThread(channel));
    console.log(`Read ${channel.name ?? threadId}: ${channel.messages?.length ?? 0} messages.`);
  }

  const exportDocument = {
    format: 1,
    exportedAt: new Date().toISOString(),
    source: {
      type: "discord-local-rpc",
      accountId: identity.user?.id,
      scopes: identity.scopes,
    },
    campaignSources: sourceMapMappings,
    threads,
  };
  await writeJson(OUTPUT_PATH, exportDocument);
  console.log(`Wrote ${threads.length} threads to ${OUTPUT_PATH}. Review it before any CMS import.`);
}

if (hasFlag("--help") || hasFlag("-h")) {
  usage();
  process.exit(0);
}

const oauthConfiguration = readOAuthConfiguration();
const rpc = await openDiscordRpc();
try {
  const identity = await authorize(rpc, oauthConfiguration);
  if (hasFlag("--list")) await createInventory(rpc, identity);
  else await exportDescriptions(rpc, identity);
} finally {
  rpc.close();
}
