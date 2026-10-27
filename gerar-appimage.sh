#!/usr/bin/env bash
# Compatibility alias for existing local packaging commands.
set -euo pipefail
exec "$(dirname -- "${BASH_SOURCE[0]}")/generate-appimage.sh" "$@"
