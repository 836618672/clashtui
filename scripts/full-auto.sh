#!/usr/bin/env bash
set -euo pipefail
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
if ! command -v cargo >/dev/null || ! command -v node >/dev/null || ! command -v python3 >/dev/null || ! command -v chromium >/dev/null || ! command -v bats >/dev/null || ! command -v qemu-system-x86_64 >/dev/null; then
  exec nix shell nixpkgs#cargo nixpkgs#rustc nixpkgs#rustfmt nixpkgs#clippy nixpkgs#gcc nixpkgs#nodejs nixpkgs#python3 nixpkgs#chromium nixpkgs#qemu nixpkgs#cdrkit nixpkgs#openssh nixpkgs#curl nixpkgs#binutils nixpkgs#bats -c bash "$script_dir/full-auto.sh" "$@"
fi
exec python3 "$script_dir/full-auto.py" "$@"
