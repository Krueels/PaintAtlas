#!/usr/bin/env bash
set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")"

# Localiza o Rust mesmo quando o gerenciador de arquivos não carrega o PATH do terminal.
if ! command -v cargo >/dev/null 2>&1; then
    export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
fi
if ! command -v cargo >/dev/null 2>&1; then
    echo "Erro: Cargo não encontrado. Instale o Rust para compilar o projeto." >&2
    exit 1
fi

echo "Compilando PaintAtlas…"
cargo build --workspace "$@"
echo "Compilação concluída. Executável em target/debug/paintatlas (ou target/release/paintatlas com --release)."
