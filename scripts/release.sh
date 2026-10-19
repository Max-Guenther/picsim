#!/usr/bin/env bash
# Neue Version veröffentlichen, z.B.: scripts/release.sh 0.2.0
#
# Setzt die Version in Cargo.toml, testet, committet, legt den Tag an und
# pusht. Den Rest (Pakete bauen, GitHub-Release) erledigt .github/workflows/release.yml.

set -euo pipefail
cd "$(dirname "$0")/.."

v="${1:?Version fehlt, z.B. 0.2.0}"
if ! grep -q "^## $v" CHANGELOG.md; then
    echo "CHANGELOG.md braucht zuerst einen Abschnitt '## $v'"
    exit 1
fi
if git rev-parse "v$v" >/dev/null 2>&1; then
    echo "Tag v$v gibt es schon"
    exit 1
fi

sed -i.bak "0,/^version = .*/s//version = \"$v\"/" Cargo.toml && rm Cargo.toml.bak
cargo test --workspace
git add -A
git commit -m "Version $v"
git tag -a "v$v" -m "PicSim $v"
git push origin main "v$v"
echo "v$v gepusht. Das Release erscheint in ein paar Minuten auf GitHub."
