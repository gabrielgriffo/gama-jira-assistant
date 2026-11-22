#!/usr/bin/env bash
set -euo pipefail

TAG="${1:?uso: release-downloads.sh <tag>}"
REPO="${REPO:-${GITHUB_REPOSITORY:?defina REPO=dono/repositorio}}"
VERSION="${TAG#v}"
APP="${APP:-Gama}"
BASE="https://github.com/$REPO/releases/download/$TAG"
START='<!-- downloads:start -->'
END='<!-- downloads:end -->'

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

block="$(printf '%s\n\n%s\n\n%s' "$START" "$(table)" "$END")"
body="$(gh release view "$TAG" -R "$REPO" --json body -q .body | tr -d '\r')"

if grep -qF "$START" <<<"$body" && grep -qF "$END" <<<"$body"; then
  new_body="$(BLOCK="$block" awk -v s="$START" -v e="$END" '
    $0 == s { print ENVIRON["BLOCK"]; skip = 1; next }
    $0 == e { skip = 0; next }
    !skip { print }
  ' <<<"$body")"
elif [ -n "$body" ]; then
  new_body="$(printf '%s\n\n%s' "$block" "$body")"
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
