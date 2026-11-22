#!/usr/bin/env bash
root="$(cd "$(dirname "$0")/../.." && pwd)"
file="$(node -e 'let d="";process.stdin.on("data",c=>d+=c).on("end",()=>{const j=JSON.parse(d||"{}");process.stdout.write((j.tool_response&&j.tool_response.filePath)||(j.tool_input&&j.tool_input.file_path)||"")})')"
[ -n "$file" ] && [ -f "$file" ] || exit 0
if command -v cygpath >/dev/null 2>&1; then
  file="$(cygpath -u "$file")"
fi
case "$file" in
  "$root"/src/*.ts | "$root"/src/*.html | "$root"/src/*.css | "$root"/src/*.json)
    cd "$root" && npx --no-install prettier --write --log-level warn "$file"
    ;;
  "$root"/src-tauri/src/*.rs)
    rustfmt --edition 2021 "$file"
    ;;
esac
