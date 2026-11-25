#!/bin/bash
# Prepare a design-stick card on Linux, instead of Raspberry Pi Imager:
#   sudo bash make-card.sh /dev/sdX IMAGE [HOSTNAME]
# IMAGE is Raspberry Pi OS Lite (64-bit), .img or .img.xz. Asks for the Wi-Fi and a password,
# writes the card (erasing it), and makes the Pi install the stick by itself on its first start.
set -euo pipefail
cd "$(dirname "$0")"
DEV=${1:-}
IMAGE=${2:-}
NAME=${3:-ecstick1}

fail() { echo "$*" >&2; exit 1; }
[ "$(id -u)" -eq 0 ] || fail "Run with sudo: sudo bash $0 $*"
[ -b "$DEV" ] && [ -f "$IMAGE" ] || fail "Usage: sudo bash $0 /dev/sdX IMAGE [HOSTNAME]"
[ "$(lsblk -dno TYPE "$DEV")" = disk ] || fail "$DEV is not a whole disk (use /dev/sdX, not /dev/sdX1)"
RM=$(lsblk -dno RM "$DEV" | tr -d ' ')
TRAN=$(lsblk -dno TRAN "$DEV" | tr -d ' ')
[ "$RM" = 1 ] || [ "$TRAN" = usb ] || [[ "$DEV" == /dev/mmcblk* ]] || fail "$DEV is not a removable card"
SIZE=$(lsblk -bdno SIZE "$DEV")
[ "$SIZE" -ge $((4 * 1024 ** 3)) ] && [ "$SIZE" -le $((256 * 1024 ** 3)) ] || fail "$DEV is not 4 to 256 GB"
if lsblk -nro MOUNTPOINTS "$DEV" | grep -qxE '/|/boot.*|/home|/usr|/var'; then
    fail "$DEV holds this computer's system"
fi
[[ "$NAME" =~ ^[a-z0-9-]{1,63}$ ]] || fail "Name: lower-case letters, digits and - only"

echo
lsblk -o NAME,SIZE,TRAN,MODEL,LABEL,MOUNTPOINTS "$DEV"
echo
read -rp "Everything on $DEV will be erased. Type its name ($(basename "$DEV")) to go on: " OK
[ "$OK" = "$(basename "$DEV")" ] || fail "Stopped."

read -rp "Wi-Fi name (2.4 GHz): " SSID
read -rsp "Wi-Fi password: " WIFIPW; echo
read -rp "Pi user name [shop]: " PIUSER
PIUSER=${PIUSER:-shop}
[[ "$PIUSER" =~ ^[a-z][a-z0-9_-]{0,31}$ ]] || fail "User name: lower-case letters and digits"
read -rsp "Pi password for $PIUSER: " PW; echo
read -rsp "Same password again: " PW2; echo
[ -n "$PW" ] && [ "$PW" = "$PW2" ] || fail "The passwords differ."
HASH=$(printf '%s' "$PW" | openssl passwd -6 -stdin)
unset PW PW2
ZONE=$(timedatectl show -p Timezone --value 2>/dev/null || true)
ZONE=${ZONE:-America/Toronto}
KEY=
if [ -n "${SUDO_USER:-}" ]; then
    HOMEDIR=$(getent passwd "$SUDO_USER" | cut -d: -f6)
    for k in "$HOMEDIR/.ssh/id_ed25519.pub" "$HOMEDIR/.ssh/id_rsa.pub"; do
        [ -f "$k" ] && { KEY=$(cat "$k"); break; }
    done
fi

echo "Writing the card…"
for part in $(lsblk -nro PATH "$DEV" | tail -n +2); do umount "$part" 2>/dev/null || true; done
case "$IMAGE" in
    *.xz) xz -dc "$IMAGE" ;;
    *) cat "$IMAGE" ;;
esac | dd of="$DEV" bs=4M conv=fsync status=progress
sync
blockdev --rereadpt "$DEV" 2>/dev/null || partprobe "$DEV" 2>/dev/null || true
udevadm settle
BOOT=$(lsblk -nro PATH "$DEV" | sed -n 2p)
[ -b "$BOOT" ] || fail "The card's first partition did not appear; take the card out and in, then try again."

MNT=$(mktemp -d)
trap 'umount "$MNT" 2>/dev/null || true; rmdir "$MNT"' EXIT
mount "$BOOT" "$MNT"
[ -f "$MNT/cmdline.txt" ] && [ -f "$MNT/user-data" ] || fail "This is not a Raspberry Pi OS image with cloud-init (2025 or later)."

# cloud-init settings for the user, SSH and the stick install. JSON strings are valid YAML, so
# anything typed is quoted safely.
export NAME PIUSER HASH ZONE KEY SSID WIFIPW
python3 - "$MNT" <<'EOF'
import json, os, sys
e, q, out = os.environ, json.dumps, sys.argv[1]
keys = f"\n  ssh_authorized_keys:\n  - {q(e['KEY'])}" if e["KEY"] else ""
user = f"""#cloud-config
hostname: {q(e['NAME'])}
manage_etc_hosts: true
timezone: {q(e['ZONE'])}
keyboard:
  model: pc105
  layout: us
users:
- name: {q(e['PIUSER'])}
  groups: users,adm,dialout,audio,netdev,video,plugdev,cdrom,games,input,gpio,spi,i2c,render,sudo
  shell: /bin/bash
  lock_passwd: false
  passwd: {q(e['HASH'])}
  sudo: ALL=(ALL) NOPASSWD:ALL{keys}
enable_ssh: true
ssh_pwauth: true
# install the design stick, then restart into it
runcmd:
- [bash, /boot/firmware/pi-stick/setup.sh]
power_state:
  mode: reboot
  message: Design stick installed, restarting
  condition: true
"""
with open(os.path.join(out, "user-data"), "w") as f:
    f.write(user)
with open(os.path.join(out, "network-config"), "w") as f:
    f.write("# Wi-Fi: see /etc/NetworkManager/system-connections/shop-wifi.nmconnection\n")
EOF
mkdir -p "$MNT/pi-stick"
cp ecstick ecstick.conf ecstick.service setup.sh send.sh nm-wifi.py README.md trimtest.dst "$MNT/pi-stick/"
umount "$MNT"
# the Wi-Fi goes straight to NetworkManager (cloud-init's netplan route can leave it empty)
ROOT=$(lsblk -nro PATH "$DEV" | sed -n 3p)
mount "$ROOT" "$MNT"
python3 nm-wifi.py "$MNT"
sync
umount "$MNT"
echo
echo "Card ready. Put it in the Pi and power it from PWR. After 3 to 5 minutes (it restarts once):"
echo "  ssh $PIUSER@$NAME.local ecstick status"
