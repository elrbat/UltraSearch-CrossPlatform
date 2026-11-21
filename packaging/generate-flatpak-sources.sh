#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

command -v cargo >/dev/null || { echo 'Rust/Cargo is required.' >&2; exit 1; }
python3 -m venv .venv-flatpak
. .venv-flatpak/bin/activate
python -m pip install --upgrade pip aiohttp tomlkit pyyaml
cargo generate-lockfile --manifest-path app/Cargo.toml
curl -fsSL https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py \
  -o /tmp/flatpak-cargo-generator.py
python /tmp/flatpak-cargo-generator.py app/Cargo.lock -o cargo-sources.json
printf 'Generated app/Cargo.lock and cargo-sources.json.\n'
