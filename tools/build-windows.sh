#!/usr/bin/env bash
# Build EmbroideryCraft for Windows (64-bit) on Linux and pack it as a zip in dist/.
# Needs: rustup, python3. Installs (once, for this user): the Windows Rust target, the zig
# linker (in ~/.cache/embcraft-zig) and cargo-zigbuild.
set -euo pipefail
cd "$(dirname "$0")/.."
ZIGENV="${HOME}/.cache/embcraft-zig"
TARGET=x86_64-pc-windows-gnu

rustup target list --installed | grep -q "^${TARGET}$" || rustup target add "${TARGET}"
[ -x "${ZIGENV}/bin/python" ] || python3 -m venv "${ZIGENV}"
"${ZIGENV}/bin/python" -c "import ziglang" 2>/dev/null || "${ZIGENV}/bin/pip" install -q ziglang
command -v cargo-zigbuild >/dev/null || cargo install cargo-zigbuild
export PATH="${ZIGENV}/bin:${PATH}"

cargo zigbuild -j 4 --release -p embcraft-gui -p embcraft --target "${TARGET}"

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
OUT="dist/EmbroideryCraft-${VERSION}-windows-x64"
rm -rf "${OUT}" "${OUT}.zip"
mkdir -p "${OUT}"
cp "target/${TARGET}/release/embcraft-gui.exe" "${OUT}/EmbroideryCraft.exe"
cp "target/${TARGET}/release/embcraft.exe" "${OUT}/embcraft.exe"
cp docs/WINDOWS.md "${OUT}/README-Windows.md"
cp LICENSE* "${OUT}/" 2>/dev/null || true
(cd dist && 7z a -tzip -bso0 "$(basename "${OUT}").zip" "$(basename "${OUT}")")
echo "Built ${OUT}.zip"
