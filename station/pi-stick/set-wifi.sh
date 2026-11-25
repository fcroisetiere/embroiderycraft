#!/bin/bash
# Change the Wi-Fi of a design-stick card, in a Linux PC's card reader (opened in the file manager):
#   sudo bash set-wifi.sh [ROOTFS] [BOOTFS]
# Also makes the Pi write wifi-report.txt on the card's boot partition at each start.
set -euo pipefail
cd "$(dirname "$0")"
fail() { echo "$*" >&2; exit 1; }
[ "$(id -u)" -eq 0 ] || fail "Run with sudo: sudo bash $0 $*"
USR=${SUDO_USER:-root}
R=${1:-/run/media/$USR/rootfs}
B=${2:-/run/media/$USR/bootfs}
[ -f "$R/usr/local/bin/ecstick" ] && [ -f "$B/config.txt" ] || fail "Card not found at $R and $B (open both in the file manager)"

read -rp "Wi-Fi name (2.4 GHz): " SSID
read -rsp "Wi-Fi password: " WIFIPW; echo
[ -n "$SSID" ] && [ ${#WIFIPW} -ge 8 ] || fail "A name and a password of 8 characters or more are needed."
export SSID WIFIPW

mount -o remount,rw "$R"
mount -o remount,rw "$B"
python3 nm-wifi.py "$R"
find "$R/etc/netplan" -name '90-NM-*.yaml' -size 0 -delete 2>/dev/null || true
install -m 755 wifi-report.sh "$R/usr/local/bin/wifi-report.sh"
install -m 644 wifi-report.service "$R/etc/systemd/system/wifi-report.service"
ln -sf /etc/systemd/system/wifi-report.service "$R/etc/systemd/system/multi-user.target.wants/wifi-report.service"
rm -f "$B/wifi-report.txt"
sync
umount "$B" "$R"
echo "Wi-Fi set to $SSID. Put the card in the Pi; after a start, wifi-report.txt on the card shows what it saw."
