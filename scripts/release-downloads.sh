#!/usr/bin/env bash
set -euo pipefail

TAG="${1:?uso: release-downloads.sh <tag>}"
REPO="${REPO:-${GITHUB_REPOSITORY:?defina REPO=dono/repositorio}}"
VERSION="${TAG#v}"
APP="${APP:-Gama}"
BASE="https://github.com/$REPO/releases/download/$TAG"
START='<!-- downloads:start -->'
END='<!-- downloads:end -->'
LOG_START='<!-- changelog:start -->'
LOG_END='<!-- changelog:end -->'

ASSETS="$(gh release view "$TAG" -R "$REPO" --json assets -q '.assets[].name')"

EXE="${APP}_${VERSION}_x64-setup.exe"
MSI="${APP}_${VERSION}_x64_en-US.msi"
DEB="${APP}_${VERSION}_amd64.deb"
RPM="${APP}-${VERSION}-1.x86_64.rpm"
APPIMAGE="${APP}_${VERSION}_amd64.AppImage"

missing=0
for file in "$EXE" "$MSI" "$DEB" "$RPM" "$APPIMAGE" latest.json; do
  if ! grep -qxF "$file" <<<"$ASSETS"; then
    echo "Artefato ausente na release: $file" >&2
    missing=1
  fi
done
if [ "$missing" = 1 ]; then
  exit 1
fi

manifest="$(gh release download "$TAG" -R "$REPO" -p latest.json -O -)"
if ! jq -e --arg v "$VERSION" '.version == $v or .version == ("v" + $v)' <<<"$manifest" >/dev/null; then
  echo "latest.json não corresponde à versão $VERSION" >&2
  exit 1
fi
for platform in windows-x86_64 linux-x86_64; do
  if ! jq -e --arg p "$platform" '(.platforms[$p].url // "") != "" and (.platforms[$p].signature // "") != ""' <<<"$manifest" >/dev/null; then
    echo "latest.json sem URL ou assinatura para $platform" >&2
    exit 1
  fi
done

link() {
  printf '[%s](%s/%s)' "$1" "$BASE" "$2"
}

table() {
  echo "| Arquitetura | Windows | Linux |"
  echo "|---|---|---|"
  echo "| x86-64 (64-bit) | $(link EXE "$EXE") $(link MSI "$MSI") | $(link DEB "$DEB") $(link RPM "$RPM") $(link AppImage "$APPIMAGE") |"
}

changelog() {
  local previous range
  previous="$(git describe --tags --abbrev=0 --match 'v[0-9]*.[0-9]*.[0-9]*' "$TAG^" 2>/dev/null || true)"
  range="$TAG"
  if [ -n "$previous" ]; then
    range="$previous..$TAG"
  fi
  echo "### Changelog"
  echo
  git log --no-merges --abbrev=7 --format='%h %H %s' "$range" | while read -r short full subject; do
    printf -- '- %s ([`%s`](https://github.com/%s/commit/%s))\n' "$subject" "$short" "$REPO" "$full"
  done
}

without_block() {
  awk -v s="$1" -v e="$2" '
    $0 == s { skip = 1; next }
    $0 == e { skip = 0; next }
    !skip { print }
  '
}

block="$(printf '%s\n\n%s\n\n%s\n\n%s\n\n%s\n\n%s' "$START" "$(table)" "$END" "$LOG_START" "$(changelog)" "$LOG_END")"
body="$(gh release view "$TAG" -R "$REPO" --json body -q .body | tr -d '\r')"
rest="$(without_block "$START" "$END" <<<"$body" | without_block "$LOG_START" "$LOG_END" | sed '/./,$!d')"

if [ -n "$rest" ]; then
  new_body="$(printf '%s\n\n%s' "$block" "$rest")"
else
  new_body="$block"
fi

if [ "${DRY_RUN:-0}" = 1 ]; then
  printf '%s\n' "$new_body"
  exit 0
fi

notes="$(mktemp)"
trap 'rm -f "$notes"' EXIT
printf '%s\n' "$new_body" > "$notes"
gh release edit "$TAG" -R "$REPO" --notes-file "$notes"
