#!/usr/bin/env bash
# Build the Shop Library (embcraft-server) download kits in dist/:
#   EmbroideryCraft-Shop-Library-<version>-linux-x64.tar.gz  (Debian 11+, Ubuntu 20.04+: glibc 2.31 or
#     newer), with its install kit from station/library-vm
#   EmbroideryCraft-Shop-Library-<version>-windows-x64.zip   (runs on one Windows PC)
# The version is the server's own (apps/embcraft-server/Cargo.toml), not the desktop app's.
# Needs: rustup, python3, 7z. Installs (once, for this user) the zig linker and cargo-zigbuild, as
# build-windows.sh does.
set -euo pipefail
cd "$(dirname "$0")/.."
ZIGENV="${HOME}/.cache/embcraft-zig"
LINUX=x86_64-unknown-linux-gnu
WIN=x86_64-pc-windows-gnu

[ -x "${ZIGENV}/bin/python" ] || python3 -m venv "${ZIGENV}"
"${ZIGENV}/bin/python" -c "import ziglang" 2>/dev/null || "${ZIGENV}/bin/pip" install -q ziglang
command -v cargo-zigbuild >/dev/null || cargo install cargo-zigbuild
export PATH="${ZIGENV}/bin:${PATH}"
rustup target add "${WIN}" >/dev/null

VERSION=$(grep -m1 '^version' apps/embcraft-server/Cargo.toml | cut -d'"' -f2)
NAME="EmbroideryCraft-Shop-Library-${VERSION}"
mkdir -p dist

# Linux
cargo zigbuild -j 4 --release -p embcraft-server --target "${LINUX}.2.31"
OUT="dist/${NAME}-linux-x64"
rm -rf "${OUT}" "${OUT}.tar.gz"
mkdir -p "${OUT}"
cp "target/${LINUX}/release/embcraft-server" "${OUT}/"
cp station/library-vm/{install.sh,nas-backup.sh,mount-clients.sh,shared-folder.sh,embcraft-library.service,README.md} "${OUT}/"
cp LICENSE-MIT LICENSE-APACHE "${OUT}/"
tar -C dist -czf "${OUT}.tar.gz" "$(basename "${OUT}")"
echo "Built ${OUT}.tar.gz"

# Windows
cargo zigbuild -j 4 --release -p embcraft-server --target "${WIN}"
OUT="dist/${NAME}-windows-x64"
rm -rf "${OUT}" "${OUT}.zip"
mkdir -p "${OUT}"
cp "target/${WIN}/release/embcraft-server.exe" "${OUT}/"
printf '%s\r\n' '@echo off' 'rem Start the Shop Library on this computer and open it in the browser.' \
    'rem Close this window to stop it.' 'cd /d "%~dp0"' 'start "" http://localhost:8080/' \
    'embcraft-server.exe --listen 127.0.0.1:8080' > "${OUT}/Start Shop Library.bat"
sed 's/$/\r/' station/windows/README.txt > "${OUT}/README.txt"
cp LICENSE-MIT LICENSE-APACHE "${OUT}/"
(cd dist && 7z a -tzip -bso0 "$(basename "${OUT}").zip" "$(basename "${OUT}")")
echo "Built ${OUT}.zip"

# Design stick files for setting up a Pi by hand (the library makes them too) and the Windows sender
OUT="dist/EmbroideryCraft-Design-Stick-${VERSION}"
rm -rf "${OUT}" "${OUT}.zip"
cp -r station/pi-stick "${OUT}"
(cd dist && 7z a -tzip -bso0 "$(basename "${OUT}").zip" "$(basename "${OUT}")")
echo "Built ${OUT}.zip"
