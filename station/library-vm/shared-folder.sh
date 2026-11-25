#!/bin/bash
# A network share on the library server that the shop's PCs save designs into (Wilcom's machine
# export straight to it, or files dragged there). The library takes each file within seconds:
# the design appears in the library tagged "shared folder", and the file leaves the share.
#
#   sudo bash shared-folder.sh                 # write-only drop box (PCs cannot see or open files)
#   sudo bash shared-folder.sh --open          # PCs can also see the files there until taken
#   sudo LIBRARY_USER=me bash shared-folder.sh # library run by another user than the service
#
# It asks for a password for the share's user, "inbox", typed on each PC once when the share is
# connected. Run it again to change the mode; the password is kept unless you change it.
set -euo pipefail
[ "$(id -u)" -eq 0 ] || exec sudo bash "$0" "$@"

DIR=/srv/library-inbox
SHARE="Library Inbox"
GROUP=libinbox
SMBUSER=inbox
LIBRARY_USER=${LIBRARY_USER:-embcraft}
MODE=dropbox
[ "${1:-}" = "--open" ] && MODE=open

id "$LIBRARY_USER" >/dev/null 2>&1 || { echo "No user $LIBRARY_USER: run install.sh first, or set LIBRARY_USER." >&2; exit 1; }

command -v smbd >/dev/null || { apt-get update -q && DEBIAN_FRONTEND=noninteractive apt-get install -y -q samba; }

getent group "$GROUP" >/dev/null || groupadd --system "$GROUP"
id "$SMBUSER" >/dev/null 2>&1 ||
    useradd --system --no-create-home --home-dir /nonexistent --shell /usr/sbin/nologin "$SMBUSER"
usermod -aG "$GROUP" "$SMBUSER"
usermod -aG "$GROUP" "$LIBRARY_USER"

mkdir -p "$DIR"
chown "$LIBRARY_USER:$GROUP" "$DIR"
# drop box: the PCs (group) may create files and folders but not list or open what is there;
# the library (owner) reads and removes them. Open: the group may also list and read.
if [ "$MODE" = dropbox ]; then DIRMODE=3730; else DIRMODE=2770; fi
chmod "$DIRMODE" "$DIR"

CONF=/etc/samba/smb.conf
# our share between markers, replaced on each run
sed -i '/^# >>> embroiderycraft inbox/,/^# <<< embroiderycraft inbox/d' "$CONF"
cat >> "$CONF" <<EOF
# >>> embroiderycraft inbox (shared-folder.sh)
[$SHARE]
   comment = Designs saved here go into the EmbroideryCraft Library
   path = $DIR
   valid users = $SMBUSER
   read only = no
   browseable = yes
   force group = $GROUP
   create mask = 0640
   force create mode = 0640
   # folders made from the PCs: the library must be able to look inside them
   directory mask = 2770
   force directory mode = 2770
   hide dot files = yes
# <<< embroiderycraft inbox
EOF
testparm -s "$CONF" >/dev/null

if ! pdbedit -L 2>/dev/null | grep -q "^$SMBUSER:"; then
    echo "Choose a password for the share (user \"$SMBUSER\"); it is typed once on each PC."
    smbpasswd -a "$SMBUSER"
elif [ "${CHANGE_PASSWORD:-}" = yes ]; then
    smbpasswd "$SMBUSER"
fi
smbpasswd -e "$SMBUSER" >/dev/null

# the library service watches the folder (a drop-in, so install.sh can update the service)
if [ -f /etc/systemd/system/embcraft-library.service ] && [ "$LIBRARY_USER" = embcraft ]; then
    mkdir -p /etc/systemd/system/embcraft-library.service.d
    cat > /etc/systemd/system/embcraft-library.service.d/shared-folder.conf <<EOF
[Service]
Environment=EMBCRAFT_INBOX=$DIR
SupplementaryGroups=$GROUP
EOF
    systemctl daemon-reload
    systemctl restart embcraft-library
fi

if command -v ufw >/dev/null && ufw status | grep -q "Status: active"; then
    ufw allow samba >/dev/null
fi
systemctl enable --now smbd >/dev/null 2>&1 || true
systemctl restart smbd

HOST=$(hostname)
echo
echo "Shared folder ready ($MODE): \\\\$HOST\\$SHARE"
echo "On each PC, in a Command Prompt (it asks for the password once):"
echo "  net use L: \"\\\\$HOST\\$SHARE\" /user:$SMBUSER /persistent:yes"
echo "Then in Wilcom: Export machine file > Tajima DST > save in L:"
if [ "$LIBRARY_USER" != embcraft ] || [ ! -f /etc/systemd/system/embcraft-library.service ]; then
    echo
    echo "Start the library with:  --inbox $DIR"
    echo "($LIBRARY_USER was added to the group $GROUP: sign out and in again, or run it with: sg $GROUP -c '...')"
fi
