#!/usr/bin/env bash
# Build a local x86_64 AppImage without installing packaging tools system-wide.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
cd "$root"
case "${1:-}" in
  --help|-h) printf 'Uso: ./gerar-appimage.sh [--skip-build]\nSaída: dist/PaintAtlas-<versão>-x86_64.AppImage\n'; exit 0 ;;
  --skip-build) skip_build=1 ;;
  '') skip_build=0 ;;
  *) printf 'Opção desconhecida: %s\n' "$1" >&2; exit 1 ;;
esac
[[ $# -le 1 ]] || { echo 'Use apenas --skip-build ou --help.' >&2; exit 1; }
arch=$(uname -m)
[[ "$arch" == x86_64 ]] || { echo 'Este script suporta Linux x86_64.' >&2; exit 1; }
for command in curl ldconfig strip sha256sum readelf git; do
  command -v "$command" >/dev/null || { printf 'Dependência ausente: %s\n' "$command" >&2; exit 1; }
done
if [[ "$skip_build" == 0 ]]; then
  cargo build --release --locked
fi
[[ -x target/release/paintatlas ]] || { echo 'Compile primeiro com cargo build --release --locked.' >&2; exit 1; }
version=$(awk -F '"' '/^version = / {print $2; exit}' Cargo.toml)
mkdir -p target/appimage-tools dist
tool="$root/target/appimage-tools/linuxdeploy-$arch.AppImage"
if [[ ! -x "$tool" ]]; then
  curl --fail --location --retry 3 --output "$tool.download" \
    "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-$arch.AppImage"
  chmod +x "$tool.download"
  mv -- "$tool.download" "$tool"
fi
stage=$(mktemp -d "$root/target/appimage-stage.XXXXXX")
trap 'rm -rf -- "$stage"' EXIT
appdir="$stage/PaintAtlas.AppDir"
mkdir -p "$appdir/usr/bin" "$appdir/usr/share/paintatlas" "$appdir/usr/share/licenses/paintatlas"
install -m755 target/release/paintatlas "$appdir/usr/bin/paintatlas"
strip --strip-unneeded "$appdir/usr/bin/paintatlas"
cp -a default_brushes "$appdir/usr/share/paintatlas/"
ln -s ../share/paintatlas/default_brushes "$appdir/usr/bin/default_brushes"
install -m644 assets/fonts/OFL.txt "$appdir/usr/share/licenses/paintatlas/Inter-OFL.txt"
# These libraries are loaded at runtime and are invisible to ldd on the Rust binary.
# OpenGL/EGL and the graphics drivers deliberately come from the host system.
libraries=(libX11.so.6 libX11-xcb.so.1 libXi.so.6 libXcursor.so.1 libXrandr.so.2
  libXinerama.so.1 libXrender.so.1 libXfixes.so.3 libXext.so.6
  libxkbcommon.so.0 libxkbcommon-x11.so.0 libwayland-client.so.0
  libwayland-cursor.so.0 libwayland-egl.so.1 libudev.so.1)
args=()
cache=$(ldconfig -p)
for library in "${libraries[@]}"; do
  path=$(awk -v name="$library" '$1 == name && /x86-64/ && !found {print $NF; found=1}' <<< "$cache")
  [[ -n "$path" ]] || { printf 'Biblioteca ausente: %s\n' "$library" >&2; exit 1; }
  args+=(--library "$path")
done
output="$root/dist/PaintAtlas-$version-$arch.AppImage"
package_output="$stage/PaintAtlas-$version-$arch.AppImage"
# Bundling curl preserves browser image URL downloads even when curl is absent on the host.
APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=1 ARCH="$arch" VERSION="$version" \
  LDAI_OUTPUT="$package_output" OUTPUT="$package_output" "$tool" \
  --appdir "$appdir" --deploy-deps-only "$appdir/usr/bin/paintatlas" \
  --executable "$(command -v curl)" "${args[@]}" \
  --desktop-file packaging/paintatlas.desktop --icon-file packaging/paintatlas.svg \
  --custom-apprun packaging/AppRun
# Preserve available distribution license notices for bundled runtime packages.
if command -v pacman >/dev/null; then
  while IFS= read -r -d '' file; do
    name=$(basename -- "$file")
    original=$(awk -v name="$name" '$1 == name && /x86-64/ && !found {print $NF; found=1}' <<< "$cache")
    [[ -n "$original" ]] || continue
    package=$(pacman -Qqo "$original" 2>/dev/null) || continue
    if [[ -d "/usr/share/licenses/$package" ]]; then
      cp -aL "/usr/share/licenses/$package" "$appdir/usr/share/licenses/"
    fi
  done < <(find "$appdir/usr/lib" -maxdepth 1 -type f -print0)
  if [[ -d /usr/share/licenses/curl ]]; then
    cp -aL /usr/share/licenses/curl "$appdir/usr/share/licenses/"
  fi
fi
APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=1 ARCH="$arch" VERSION="$version" \
  LDAI_OUTPUT="$package_output" OUTPUT="$package_output" "$tool" --appdir "$appdir" --custom-apprun packaging/AppRun --output appimage
# Rename only after success: an already running AppImage keeps its original inode.
chmod +x "$package_output"
mv -- "$package_output" "$output"
(cd dist && sha256sum "$(basename -- "$output")" > "$(basename -- "$output").sha256")
{
  printf 'PaintAtlas %s (%s)\n' "$version" "$arch"
  printf 'Built on: '; getconf GNU_LIBC_VERSION
  printf 'Required GLIBC symbols: '
  readelf --version-info target/release/paintatlas | sed -n 's/.*Name: \(GLIBC_[0-9.]*\).*/\1/p' | sort -Vu | tail -1
  printf 'Source: '; git rev-parse --short HEAD
  printf 'linuxdeploy SHA256: '; sha256sum "$tool" | awk '{print $1}'
} > "$output.build-info.txt"
printf '\nAppImage criado: %s\n' "$output"
