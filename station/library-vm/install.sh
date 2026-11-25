#!/usr/bin/env bash
# Install or update the EmbroideryCraft Library on this Linux machine (Debian 11+, Ubuntu 20.04+).
# Run from the unpacked kit:  sudo ./install.sh
# Running it again updates the program and restarts it; the library itself is kept.
set -euo pipefail
cd "$(dirname "$0")"
[ "$(id -u)" = 0 ] || { echo "Run it as root: sudo ./install.sh"; exit 1; }
PORT="${PORT:-8080}"
DATA=/var/lib/embcraft-library

id embcraft >/dev/null 2>&1 ||
    useradd --system --home-dir "${DATA}" --no-create-home --shell /usr/sbin/nologin embcraft
install -d -o embcraft -g embcraft -m 750 "${DATA}"
install -m 755 embcraft-server /usr/local/bin/embcraft-server
sed "s/0\.0\.0\.0:8080/0.0.0.0:${PORT}/" embcraft-library.service > /etc/systemd/system/embcraft-library.service
systemctl daemon-reload
systemctl enable --quiet embcraft-library
systemctl restart embcraft-library
sleep 1
if systemctl is-active --quiet embcraft-library; then
    IP=$(hostname -I 2>/dev/null | awk '{print $1}')
    echo "EmbroideryCraft Library is running: http://${IP:-this-machine}:${PORT}/"
    echo "Library folder: ${DATA}"
    echo "First time: open that address to create the administrator, then set Settings › Backups."
else
    echo "The library did not start. See: journalctl -u embcraft-library -n 50"
    exit 1
fi
