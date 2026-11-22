#!/usr/bin/env bash
# Posts a version's changelog to a Discord webhook as one embed linking to its
# GitHub release. An embed description holds 4096 characters, so a longer
# changelog is cut at a line break and links to the rest.
#
# Env: VERSION, WEBHOOK, GITHUB_REPOSITORY
set -euo pipefail

jq -n \
  --arg title "BARAS v$VERSION" \
  --arg url "https://github.com/$GITHUB_REPOSITORY/releases/tag/v$VERSION" \
  --arg body "$(tail -n +3 "changelogs/v$VERSION.md")" '
  ($body
    | if length > 3900
      then .[:3900] | sub("\n[^\n]*$"; "") + "\n\n[Full release notes](\($url))"
      else . end) as $description
  | { embeds: [{ title: $title, url: $url, description: $description }] }' \
  | curl --fail-with-body -sS -H "Content-Type: application/json" -d @- "$WEBHOOK"
