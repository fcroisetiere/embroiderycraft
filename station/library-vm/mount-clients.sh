#!/usr/bin/env bash
# Mount the NAS customer folders read-only, to import designs into the library:
#   sudo ./mount-clients.sh 'user@DOMAIN' [//192.168.1.10/Dossier Clients]
# Asks for the password; nothing is saved, and the share is gone after a restart (or
# sudo umount /mnt/dossier-clients). Then:
#   sudo -u embcraft embcraft-server import /mnt/dossier-clients --customers --only dst,tbf,emb \
#       --data /var/lib/embcraft-library
set -euo pipefail
[ "$(id -u)" = 0 ] || { echo "Run it as root: sudo ./mount-clients.sh user@DOMAIN"; exit 1; }
[ $# -ge 1 ] || { echo "Usage: sudo ./mount-clients.sh user@DOMAIN [//nas/share]"; exit 1; }
SHARE="${2:-//192.168.1.10/Dossier Clients}"
USER_NAME="${1%@*}"
DOMAIN=""
[ "$1" != "${USER_NAME}" ] && DOMAIN="${1#*@}"
MNT=/mnt/dossier-clients

command -v mount.cifs >/dev/null || { apt-get update -q && apt-get install -y -q cifs-utils; }
mkdir -p "${MNT}"
mountpoint -q "${MNT}" && { echo "Already mounted at ${MNT}."; exit 0; }
read -r -s -p "Password for ${1} on ${SHARE}: " PASS
echo
OPTS="ro,username=${USER_NAME},uid=$(id -u embcraft),gid=$(id -g embcraft),iocharset=utf8,vers=3.0"
[ -n "${DOMAIN}" ] && OPTS="${OPTS},domain=${DOMAIN}"
# the password goes through the environment, not the command line (which others can read)
PASSWD="${PASS}" mount -t cifs "${SHARE}" "${MNT}" -o "${OPTS}"
unset PASS
echo "Mounted ${SHARE} at ${MNT}, read-only: $(ls "${MNT}" | wc -l) folders."
