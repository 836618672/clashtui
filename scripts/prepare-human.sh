#!/usr/bin/env bash
# This touches only the disposable guest and prepares an inactive H06 profile.
set -euo pipefail
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
cd "$script_dir/.."
mkdir -p target
exec 9>target/acceptance.lock
flock -n 9 || { echo 'An acceptance run is active; prepare human checks after it finishes.' >&2; exit 1; }
bash scripts/vm.sh check
bash scripts/vm.sh ssh 'mkdir -p /home/tester/clashtui-test/manual; cat > /home/tester/clashtui-test/manual/lab.py' < scripts/manual-test-lab.py
bash scripts/vm.sh ssh 'python3 -' < scripts/prepare-human.py
