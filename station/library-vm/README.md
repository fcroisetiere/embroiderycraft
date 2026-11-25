# EmbroideryCraft Shop Library on a Proxmox VM (beta)

The library runs as a service on a small Linux VM (or any Debian 11+/Ubuntu 20.04+ machine), and
everyone in the shop opens it in the browser at `http://<vm address>:8080/`.

## 1. Make the VM in Proxmox

- **OS:** Debian 12 or 13 (netinst ISO), no desktop, with "SSH server" ticked.
- **CPU and memory:** 2 cores, 2 GB.
- **Disk:** 32 GB. The library takes about twice the size of the design files: 16,000 designs is
  about 1 GB with previews.
- **Network:** a fixed address, either a DHCP reservation on the router or a static IP set during
  the install, so the address in everyone's bookmarks doesn't change.
- **Options:** turn on the QEMU guest agent, and *Start at boot*.

An LXC container works too and is lighter. To mount the NAS for backups (step 4), it must be
privileged, or the share must be mounted on the Proxmox host and passed in as a mount point.

## 2. Build the kit (on the Linux workstation)

```
bash tools/build-shop-library.sh
```
This makes `dist/EmbroideryCraft-Shop-Library-<version>-linux-x64.tar.gz` (and the Windows zip). You can also download the kit from the website: https://embroiderycraft.org/shop-library.html

## 3. Install

```
scp dist/EmbroideryCraft-Shop-Library-*-linux-x64.tar.gz you@<vm address>:
ssh you@<vm address>
tar xzf EmbroideryCraft-Shop-Library-*-linux-x64.tar.gz
cd EmbroideryCraft-Shop-Library-*-linux-x64
sudo ./install.sh
```
Open the address it prints and create the administrator. To use another port, run
`sudo PORT=8090 ./install.sh`.

To **update** later, copy the new kit over and run `sudo ./install.sh` again. The library is kept.

## 4. Backups to the NAS

```
sudo ./nas-backup.sh '//192.168.1.10/Backups/EmbroideryCraft Library' 'user@DOMAIN'
```
This asks for the password, mounts the share at `/mnt/library-backup` on every start, and checks
it can write there. While the share is not mounted, nothing can be written in that folder: backups
fail and say so, instead of quietly filling the VM's disk. Then, in the library, go to
**Settings › Backups** and set the folder to `/mnt/library-backup`, every day.

Proxmox's own VM backups (to a Proxmox Backup Server or other storage) are a good second layer.
They restore the whole VM, while the library backup restores the designs anywhere, even without
Proxmox.

## 5. Move an existing library onto the VM

On the computer running the library now, back it up to the NAS. Use **Back up now** in Settings ›
Backups, or stop it and run `embcraft-server backup <NAS folder>`. Then on the VM, after steps 3
and 4:

```
sudo systemctl stop embcraft-library
sudo mv /var/lib/embcraft-library /var/lib/embcraft-library.empty     # the new, empty library
sudo install -d -o embcraft -g embcraft -m 750 /var/lib/embcraft-library
sudo -u embcraft embcraft-server restore /mnt/library-backup --data /var/lib/embcraft-library
sudo systemctl start embcraft-library
```
People, passwords, customers, colourways and settings come along. Previews are drawn again in the
first minutes. Once it's working, remove `/var/lib/embcraft-library.empty`.

## 6. Shared folder for Wilcom (optional)

A network share the PCs save designs into. In Wilcom, open the EMB, then **Export machine file ›
Tajima DST** and save it in the share instead of on a USB stick. Within a few seconds the design
is in the library, tagged **shared folder**, and the file has left the share. Save the EMB there
too (File › Save As) and it is kept with the DST, untouched, as the master.

```
sudo bash shared-folder.sh
```
It asks for a password for the share's user, `inbox`. Then on each PC, once, in a Command Prompt
(it asks for that password):
```
net use L: "\\<VM name>\Library Inbox" /user:inbox /persistent:yes
```
The share is a **drop box**: the PCs can save into it but not see or open what is there (the
library takes everything anyway). If Wilcom refuses to save into a folder it cannot list, run
`sudo bash shared-folder.sh --open` instead.

A machine file that does not read (an export cut short) is not imported: it waits in
`/srv/library-inbox/Not imported` with a note saying why. Saving the same DST again does not make a
second design; it tags the existing one again so it shows with the new ones.

## Everyday commands (on the VM)

| What | Command |
|---|---|
| Is it running? | `systemctl status embcraft-library` |
| Its messages | `journalctl -u embcraft-library -f` |
| Restart | `sudo systemctl restart embcraft-library` |
| Forgotten password | `sudo -u embcraft embcraft-server password <user> --data /var/lib/embcraft-library` |
| Shared folder messages | `journalctl -u embcraft-library \| grep -i shared` |
| Import a folder | `sudo -u embcraft embcraft-server import <folder> --customers --data /var/lib/embcraft-library` |

The library speaks plain HTTP. That is fine inside the shop network, but don't forward the port
on the router to the internet.
