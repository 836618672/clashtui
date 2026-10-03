#!/usr/bin/env bash
set -euo pipefail
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
if ! command -v qemu-system-x86_64 >/dev/null || ! command -v genisoimage >/dev/null || ! command -v python3 >/dev/null; then
  exec nix shell nixpkgs#qemu nixpkgs#cdrkit nixpkgs#openssh nixpkgs#curl nixpkgs#python3 nixpkgs#binutils -c bash "$script_dir/vm.sh" "$@"
fi
exec python3 "$script_dir/vm.py" "$@"
