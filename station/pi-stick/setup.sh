#!/bin/bash
# Install the design stick on Raspberry Pi OS Lite (Pi Zero 2 W). Run once, then reboot:
#   sudo bash setup.sh
set -euo pipefail
cd "$(dirname "$0")"
[ "$(id -u)" -eq 0 ] || exec sudo bash "$0" "$@"

CFG=/boot/firmware/config.txt
[ -f "$CFG" ] || CFG=/boot/config.txt
# look for our own line: the stock config.txt already has dtoverlay=dwc2 for the CM5 only
if ! grep -q '^dtoverlay=dwc2,dr_mode=peripheral' "$CFG"; then
    printf '\n[all]\n# USB port as a device (the Pi is the stick)\ndtoverlay=dwc2,dr_mode=peripheral\n' >> "$CFG"
fi
printf 'dwc2\nlibcomposite\n' > /etc/modules-load.d/ecstick.conf
# name.local gives the IPv4 address (PCs often cannot reach the Pi's IPv6 one), and Wi-Fi power
# saving off so the Pi answers at once
sed -i 's/^#\?use-ipv6=.*/use-ipv6=no/' /etc/avahi/avahi-daemon.conf
# the languages a Mac or PC asks for over ssh (avoids "setlocale" warnings)
sed -i -E 's/^# *((en_US|en_CA|fr_CA)\.UTF-8 UTF-8)/\1/' /etc/locale.gen
locale-gen >/dev/null
mkdir -p /etc/NetworkManager/conf.d
printf '[connection]\nwifi.powersave = 2\n' > /etc/NetworkManager/conf.d/ecstick-powersave.conf

install -m 755 ecstick /usr/local/bin/ecstick
[ -f /etc/ecstick.conf ] || install -m 644 ecstick.conf /etc/ecstick.conf
install -m 644 ecstick.service /etc/systemd/system/ecstick.service
systemctl daemon-reload
systemctl enable ecstick.service
mkdir -p /var/lib/ecstick
echo
echo "Installed. Now: sudo reboot"
echo "After the reboot, plug the Pi's USB port (not PWR) into the machine, then: ecstick status"
