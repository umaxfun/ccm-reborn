#!/usr/bin/env bash

# Publish one CCM ZIP and make it available through the production catalog.
# The archive is uploaded before catalog.json, so users never receive a
# manifest that links to an unavailable package.
set -euo pipefail

REPOSITORY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CATALOG_PATH="$REPOSITORY_ROOT/catalog/catalog.json"
R2_BUCKET="ccm-reborn"
PUBLIC_BASE_URL="https://files.ccm-reborn.mikilabs.io"
ARCHIVE_CACHE_CONTROL="public, max-age=31536000, immutable"
CATALOG_CACHE_CONTROL="no-store"
WRANGLER_MAX_BYTES=$((315 * 1024 * 1024))

usage() {
  printf '%s\n' \
    'Usage:' \
    '  npm run catalog:publish -- /path/to/campaign.zip --slug short-name [options]' \
    '' \
    'Required:' \
    '  --slug NAME          Short URL name, for example artanis-rogue.' \
    '' \
    'Optional overrides for metadata.txt:' \
    '  --id ID              Catalog ID; defaults to --slug.' \
    '  --version VERSION    URL/catalog version; defaults to metadata.txt.' \
    '  --title TEXT         Display title.' \
    '  --author TEXT        Display author.' \
    '  --description TEXT   Display description.' \
    '  --campaign BRANCH    wol, hots, lotv, or nco.' \
    '  --dry-run            Validate and show the planned publication only.' \
    '' \
    'The script reads metadata.txt, verifies the ZIP, uploads its immutable' \
    'R2 object, stores a catalog-history copy, then publishes catalog.json last.'
}

fail() {
  printf 'Publish failed: %s\n' "$*" >&2
  exit 1
}

require_value() {
  [ "$#" -ge 2 ] || fail "$1 needs a value."
}

trim() {
  printf '%s' "$1" | sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//'
}

normalise_version() {
  printf '%s' "$1" \
    | sed -E 's/[[:space:]]+[Oo]fficial[[:space:]]+[Rr]elease$//; s/[[:space:]]+[Rr]elease$//; s/^[[:space:]]+//; s/[[:space:]]+$//'
}

metadata_field() {
  awk -v key="$1" 'index($0, key "=") == 1 { print substr($0, length(key) + 2); exit }' "$METADATA_TEXT"
}

canonical_json() {
  jq -S . "$1"
}

same_json() {
  cmp -s <(canonical_json "$1") <(canonical_json "$2")
}

same_catalog_content() {
  cmp -s \
    <(jq -S 'del(.updatedAt)' "$1") \
    <(jq -S 'del(.updatedAt)' "$2")
}

probe_url() {
  printf '%s?ccm-publish-probe=%s-%s' "$1" "$(date +%s)" "$RANDOM"
}

head_status() {
  curl --silent --show-error --location --head --output /dev/null --write-out '%{http_code}' "$(probe_url "$1")"
}

head_size() {
  curl --silent --show-error --location --head "$(probe_url "$1")" \
    | tr -d '\r' \
    | awk 'tolower($1) == "content-length:" { size = $2 } END { print size }'
}

download_public() {
  curl --fail --silent --show-error --location --header 'Cache-Control: no-cache' "$1" --output "$2"
}

put_object() {
  local key="$1"
  local file="$2"
  local cache_control="$3"
  local content_type="application/json"
  [[ "$key" == *.zip ]] && content_type="application/zip"
  wrangler r2 object put "$R2_BUCKET/$key" \
    --remote \
    --file "$file" \
    --content-type "$content_type" \
    --cache-control "$cache_control"
}

campaign_from_metadata() {
  local value
  value="$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | tr -d '[:space:]')"
  case "$value" in
    wol|wings|liberty|wingsofliberty) printf 'wol|Wings of Liberty' ;;
    hots|heart|swarm|heartoftheswarm) printf 'hots|Heart of the Swarm' ;;
    lotv|legacy|void|legacyofthevoid) printf 'lotv|Legacy of the Void' ;;
    nco|nova|covert|novacovertops) printf 'nco|Nova Covert Ops' ;;
    *) fail "metadata.txt has an unknown campaign=$1; use --campaign wol|hots|lotv|nco." ;;
  esac
}

campaign_from_option() {
  case "$1" in
    wol) printf 'wol|Wings of Liberty' ;;
    hots) printf 'hots|Heart of the Swarm' ;;
    lotv) printf 'lotv|Legacy of the Void' ;;
    nco) printf 'nco|Nova Covert Ops' ;;
    *) fail "--campaign must be wol, hots, lotv, or nco." ;;
  esac
}

ARCHIVE=""
SLUG=""
CAMPAIGN_ID=""
VERSION_OVERRIDE=""
TITLE_OVERRIDE=""
AUTHOR_OVERRIDE=""
DESCRIPTION_OVERRIDE=""
CAMPAIGN_OVERRIDE=""
DRY_RUN=false

while [ "$#" -gt 0 ]; do
  case "$1" in
    --help|-h)
      usage
      exit 0
      ;;
    --slug)
      require_value "$@"
      SLUG="$2"
      shift 2
      ;;
    --id)
      require_value "$@"
      CAMPAIGN_ID="$2"
      shift 2
      ;;
    --version)
      require_value "$@"
      VERSION_OVERRIDE="$2"
      shift 2
      ;;
    --title)
      require_value "$@"
      TITLE_OVERRIDE="$2"
      shift 2
      ;;
    --author)
      require_value "$@"
      AUTHOR_OVERRIDE="$2"
      shift 2
      ;;
    --description)
      require_value "$@"
      DESCRIPTION_OVERRIDE="$2"
      shift 2
      ;;
    --campaign)
      require_value "$@"
      CAMPAIGN_OVERRIDE="$2"
      shift 2
      ;;
    --dry-run)
      DRY_RUN=true
      shift
      ;;
    --*)
      fail "Unknown option: $1"
      ;;
    *)
      [ -z "$ARCHIVE" ] || fail "Only one ZIP path may be supplied."
      ARCHIVE="$1"
      shift
      ;;
  esac
done

[ -n "$ARCHIVE" ] || { usage >&2; exit 1; }
[ -n "$SLUG" ] || fail "--slug is required."
[ -f "$ARCHIVE" ] || fail "ZIP was not found: $ARCHIVE"
[ -f "$CATALOG_PATH" ] || fail "Production catalog was not found: $CATALOG_PATH"
[[ "$SLUG" =~ ^[a-z0-9]+(-[a-z0-9]+)*$ ]] || fail "--slug must use lowercase letters, digits, and single dashes."

CAMPAIGN_ID="${CAMPAIGN_ID:-$SLUG}"
[[ "$CAMPAIGN_ID" =~ ^[a-z0-9][a-z0-9_-]{0,79}$ ]] || fail "--id may use lowercase letters, digits, dashes, and underscores."

for command in curl jq shasum stat unzip iconv xxd wrangler; do
  command -v "$command" >/dev/null 2>&1 || fail "Required command is unavailable: $command"
done

TEMPORARY_DIRECTORY="$(mktemp -d "${TMPDIR:-/tmp}/ccm-publish.XXXXXX")"
trap 'rm -rf "$TEMPORARY_DIRECTORY"' EXIT
METADATA_BYTES="$TEMPORARY_DIRECTORY/metadata.bin"
METADATA_TEXT="$TEMPORARY_DIRECTORY/metadata.txt"
REMOTE_CATALOG="$TEMPORARY_DIRECTORY/remote-catalog.json"
CANDIDATE_CATALOG="$TEMPORARY_DIRECTORY/catalog.json"

unzip -tqq "$ARCHIVE" >/dev/null || fail "ZIP integrity check failed."
METADATA_ENTRY=""
METADATA_COUNT=0
while IFS= read -r entry; do
  if [ "$(basename "$entry" | tr '[:upper:]' '[:lower:]')" = 'metadata.txt' ]; then
    METADATA_ENTRY="$entry"
    METADATA_COUNT=$((METADATA_COUNT + 1))
  fi
done < <(unzip -Z1 "$ARCHIVE")
[ "$METADATA_COUNT" -eq 1 ] || fail "ZIP must contain exactly one metadata.txt; found $METADATA_COUNT."
unzip -p "$ARCHIVE" "$METADATA_ENTRY" > "$METADATA_BYTES"

case "$(xxd -p -l 2 "$METADATA_BYTES" | tr -d '\n')" in
  fffe) iconv -f UTF-16LE -t UTF-8 "$METADATA_BYTES" > "$METADATA_TEXT" ;;
  feff) iconv -f UTF-16BE -t UTF-8 "$METADATA_BYTES" > "$METADATA_TEXT" ;;
  *) cp "$METADATA_BYTES" "$METADATA_TEXT" ;;
esac
tr -d '\r' < "$METADATA_TEXT" > "$METADATA_TEXT.normalised"
mv "$METADATA_TEXT.normalised" "$METADATA_TEXT"

TITLE="$(trim "${TITLE_OVERRIDE:-$(metadata_field title)}")"
AUTHOR="$(trim "${AUTHOR_OVERRIDE:-$(metadata_field author)}")"
DESCRIPTION="$(trim "${DESCRIPTION_OVERRIDE:-$(metadata_field desc)}")"
VERSION="$(normalise_version "${VERSION_OVERRIDE:-$(metadata_field version)}")"
METADATA_CAMPAIGN="$(trim "$(metadata_field campaign)")"
[ -n "$TITLE" ] || fail "Campaign title is missing; use --title."
[ -n "$AUTHOR" ] || fail "Campaign author is missing; use --author."
[ -n "$DESCRIPTION" ] || fail "Campaign description is missing; use --description."
[ -n "$VERSION" ] || fail "Campaign version is missing; use --version."
[[ "$VERSION" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || fail "Version $VERSION is not URL-safe; use --version (for example 1.07)."

CAMPAIGN_PAIR="$(campaign_from_metadata "$METADATA_CAMPAIGN")"
if [ -n "$CAMPAIGN_OVERRIDE" ]; then
  CAMPAIGN_PAIR="$(campaign_from_option "$CAMPAIGN_OVERRIDE")"
fi
BRANCH_KEY="${CAMPAIGN_PAIR%%|*}"
BRANCH_TITLE="${CAMPAIGN_PAIR#*|}"
OBJECT_KEY="campaigns/$BRANCH_KEY/$SLUG-$VERSION.zip"
OBJECT_URL="$PUBLIC_BASE_URL/$OBJECT_KEY"
ARCHIVE_SIZE="$(stat -f '%z' "$ARCHIVE")"
ARCHIVE_SHA256="$(shasum -a 256 "$ARCHIVE" | awk '{print $1}')"

[ "$ARCHIVE_SIZE" -le "$WRANGLER_MAX_BYTES" ] || fail "ZIP is larger than Wrangler's 315 MiB upload limit; use a configured multipart uploader for $OBJECT_KEY."
jq -e '.format == 1 and (.campaigns | type == "array")' "$CATALOG_PATH" >/dev/null \
  || fail "Production catalog is not a format-1 catalog."

download_public "$PUBLIC_BASE_URL/catalog.json" "$REMOTE_CATALOG"
same_json "$CATALOG_PATH" "$REMOTE_CATALOG" \
  || fail "Local catalog/catalog.json differs from the public catalog. Synchronize it before publishing so no remote changes are overwritten."

jq \
  --arg id "$CAMPAIGN_ID" \
  --arg title "$TITLE" \
  --arg author "$AUTHOR" \
  --arg version "$VERSION" \
  --arg description "$DESCRIPTION" \
  --arg branch "$BRANCH_TITLE" \
  --arg url "$OBJECT_URL" \
  --arg sha256 "$ARCHIVE_SHA256" \
  --argjson size "$ARCHIVE_SIZE" '
  .updatedAt = ""
  | del(.campaigns[].requirements.platforms)
  | {
      id: $id,
      title: $title,
      author: $author,
      version: $version,
      description: $description,
      tags: [$branch, "CCM package"],
      requirements: { campaign: $branch },
      package: { url: $url, sha256: $sha256, size: $size }
    } as $entry
  | .campaigns |= (
      if any(.[]; .id == $id) then
        map(if .id == $id then $entry else . end)
      else
        . + [$entry]
      end
    )
' "$CATALOG_PATH" > "$CANDIDATE_CATALOG"

if same_catalog_content "$CATALOG_PATH" "$CANDIDATE_CATALOG"; then
  UPDATED_AT="$(jq -r '.updatedAt' "$CATALOG_PATH")"
else
  UPDATED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
fi
jq --arg updated_at "$UPDATED_AT" '.updatedAt = $updated_at' "$CANDIDATE_CATALOG" > "$CANDIDATE_CATALOG.updated"
mv "$CANDIDATE_CATALOG.updated" "$CANDIDATE_CATALOG"

jq -e '
  (.campaigns | map(.id) | length) == (.campaigns | map(.id) | unique | length)
  and ([.campaigns[].requirements | has("platforms")] | any | not)
  and ([.campaigns[].package | has("url") and (has("path") | not)] | all)
' "$CANDIDATE_CATALOG" >/dev/null || fail "Generated catalog did not pass validation."

printf 'Campaign: %s by %s\n' "$TITLE" "$AUTHOR"
printf 'Package:  %s\n' "$OBJECT_URL"
printf 'Version:  %s\n' "$VERSION"
printf 'SHA-256:  %s\n' "$ARCHIVE_SHA256"
printf 'Size:     %s bytes\n' "$ARCHIVE_SIZE"

REMOTE_STATUS="$(head_status "$OBJECT_URL")"
case "$REMOTE_STATUS" in
  200)
    CATALOG_OBJECT_SHA="$(jq -r --arg url "$OBJECT_URL" '[.campaigns[] | select(.package.url == $url) | .package.sha256] | unique | .[0] // empty' "$CATALOG_PATH")"
    CATALOG_OBJECT_SIZE="$(jq -r --arg url "$OBJECT_URL" '[.campaigns[] | select(.package.url == $url) | .package.size] | unique | .[0] // empty' "$CATALOG_PATH")"
    if [ "$CATALOG_OBJECT_SHA" = "$ARCHIVE_SHA256" ] && [ "$CATALOG_OBJECT_SIZE" = "$ARCHIVE_SIZE" ]; then
      printf 'Archive already published; it matches the local catalog.\n'
    else
      EXISTING_ARCHIVE="$TEMPORARY_DIRECTORY/existing.zip"
      printf 'Archive URL already exists; verifying it before reuse.\n'
      download_public "$OBJECT_URL" "$EXISTING_ARCHIVE"
      EXISTING_SHA256="$(shasum -a 256 "$EXISTING_ARCHIVE" | awk '{print $1}')"
      EXISTING_SIZE="$(stat -f '%z' "$EXISTING_ARCHIVE")"
      [ "$EXISTING_SHA256" = "$ARCHIVE_SHA256" ] && [ "$EXISTING_SIZE" = "$ARCHIVE_SIZE" ] \
        || fail "R2 object $OBJECT_KEY already exists with different content; choose a new --version or --slug."
      printf 'Archive already exists and matches; resuming publication.\n'
    fi
    ;;
  404)
    if [ "$DRY_RUN" = true ]; then
      printf 'Dry run: archive would be uploaded.\n'
    else
      printf 'Uploading archive…\n'
      put_object "$OBJECT_KEY" "$ARCHIVE" "$ARCHIVE_CACHE_CONTROL"
      [ "$(head_status "$OBJECT_URL")" = 200 ] || fail "Archive upload completed but object is not public at $OBJECT_URL."
      [ "$(head_size "$OBJECT_URL")" = "$ARCHIVE_SIZE" ] || fail "Public archive has an unexpected size after upload."
    fi
    ;;
  *)
    fail "Could not check $OBJECT_URL: HTTP $REMOTE_STATUS."
    ;;
esac

HISTORY_KEY="catalog-history/$(printf '%s' "$UPDATED_AT" | tr ':.' '-').json"
HISTORY_URL="$PUBLIC_BASE_URL/$HISTORY_KEY"
if [ "$DRY_RUN" = true ]; then
  printf 'Dry run: would publish %s and catalog.json last.\n' "$HISTORY_KEY"
  exit 0
fi

HISTORY_STATUS="$(head_status "$HISTORY_URL")"
case "$HISTORY_STATUS" in
  200)
    download_public "$HISTORY_URL" "$TEMPORARY_DIRECTORY/history.json"
    same_json "$CANDIDATE_CATALOG" "$TEMPORARY_DIRECTORY/history.json" \
      || fail "Existing history object $HISTORY_KEY has different content."
    printf 'Catalog history already published.\n'
    ;;
  404)
    printf 'Publishing catalog history…\n'
    put_object "$HISTORY_KEY" "$CANDIDATE_CATALOG" "$ARCHIVE_CACHE_CONTROL"
    ;;
  *)
    fail "Could not check $HISTORY_URL: HTTP $HISTORY_STATUS."
    ;;
esac

download_public "$PUBLIC_BASE_URL/catalog.json" "$TEMPORARY_DIRECTORY/current-catalog.json"
if same_json "$CANDIDATE_CATALOG" "$TEMPORARY_DIRECTORY/current-catalog.json"; then
  printf 'Public catalog is already current.\n'
else
  printf 'Publishing catalog.json…\n'
  put_object 'catalog.json' "$CANDIDATE_CATALOG" "$CATALOG_CACHE_CONTROL"
  download_public "$PUBLIC_BASE_URL/catalog.json" "$TEMPORARY_DIRECTORY/published-catalog.json"
  same_json "$CANDIDATE_CATALOG" "$TEMPORARY_DIRECTORY/published-catalog.json" \
    || fail 'Public catalog differs after upload; local catalog was left unchanged.'
fi

cp "$CANDIDATE_CATALOG" "$CATALOG_PATH"
printf 'Done. Updated %s and %s.\n' "$OBJECT_KEY" "$CATALOG_PATH"
