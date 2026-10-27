#!/usr/bin/env bash
# Compatibility alias for existing local launchers.
set -euo pipefail
exec "$(dirname -- "${BASH_SOURCE[0]}")/compile.sh" "$@"
