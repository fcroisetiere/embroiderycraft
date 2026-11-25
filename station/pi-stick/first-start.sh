#!/bin/bash
# Run once by cloud-init on the first start of a card prepared by the Library (Machines › Prepare
# a design stick): installs the stick, keeps the Wi-Fi report, and takes the Wi-Fi password out of
# the card's boot partition (it stays only in NetworkManager, readable by root). Then the Pi restarts.
#   bash first-start.sh COUNTRY NAME
set -uo pipefail
cd "$(dirname "$0")"
COUNTRY=${1:-CA}
NAME=${2:-ecstick}
BOOT=/boot/firmware

# Wi-Fi is blocked until the country is known
raspi-config nonint do_wifi_country "$COUNTRY" 2>/dev/null || iw reg set "$COUNTRY" 2>/dev/null || true
rfkill unblock wifi 2>/dev/null || true

bash setup.sh

# wifi-report.txt on the card after each start, to see from any PC why the Wi-Fi does not connect
install -m 755 wifi-report.sh /usr/local/bin/wifi-report.sh
install -m 644 wifi-report.service /etc/systemd/system/wifi-report.service
systemctl enable wifi-report.service 2>/dev/null || true

# the same settings without the Wi-Fi password and keys: cloud-init does not run them again
# (same instance), but the name must stay
cat > "$BOOT/user-data" <<EOF
#cloud-config
# Design stick $NAME, set up on $(date '+%Y-%m-%d %H:%M'). The Wi-Fi password was taken out of
# this file. To set the stick up again, prepare a new card in the Library.
hostname: $NAME
manage_etc_hosts: true
EOF
echo "$(date '+%Y-%m-%d %H:%M') design stick $NAME installed" > "$BOOT/stick-installed.txt"
sync
