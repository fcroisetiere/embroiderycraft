#!/usr/bin/env bash
# Mount a NAS share (SMB) at /mnt/library-backup for the library's backups.
#   sudo ./nas-backup.sh //192.168.1.10/Backups/Library 'user@DOMAIN'
# Asks for the password and keeps it in /root/.library-nas (only root can read it).
# Then in the library: Settings › Backups › folder /mnt/library-backup.
set -euo pipefail
[ "$(id -u)" = 0 ] || { echo "Run it as root: sudo ./nas-backup.sh //nas/share/folder user"; exit 1; }
[ $# = 2 ] || { echo "Usage: sudo ./nas-backup.sh //nas/share/folder user"; exit 1; }
SHARE="$1"
USER_NAME="${2%@*}"
DOMAIN=""
[ "$2" != "${USER_NAME}" ] && DOMAIN="${2#*@}"
MNT=/mnt/library-backup
CRED=/root/.library-nas

command -v mount.cifs >/dev/null || { apt-get update -q && apt-get install -y -q cifs-utils; }
read -r -s -p "Password for ${2} on ${SHARE}: " PASS
echo
umask 077
{
    echo "username=${USER_NAME}"
    echo "password=${PASS}"
    [ -n "${DOMAIN}" ] && echo "domain=${DOMAIN}"
} > "${CRED}"
mkdir -p "${MNT}"
# while the share is not mounted, nothing can be written here: backups fail and say so,
# instead of filling the VM's own disk
mountpoint -q "${MNT}" || chattr +i "${MNT}" 2>/dev/null || true
UIDS="uid=$(id -u embcraft),gid=$(id -g embcraft)"
LINE="${SHARE// /\\040} ${MNT} cifs credentials=${CRED},${UIDS},file_mode=0640,dir_mode=0750,_netdev,nofail,x-systemd.automount 0 0"
grep -v " ${MNT} " /etc/fstab > /etc/fstab.new || true
echo "${LINE}" >> /etc/fstab.new
chmod 644 /etc/fstab.new
mv /etc/fstab.new /etc/fstab
systemctl daemon-reload
systemctl restart remote-fs.target
if touch "${MNT}/.embcraft-test" 2>/dev/null; then
    rm -f "${MNT}/.embcraft-test"
    echo "Mounted ${SHARE} at ${MNT}. In the library: Settings › Backups › folder ${MNT}"
else
    echo "Could not write to ${SHARE}. Check the path, user and password, then run this again."
    exit 1
fi
