# Design stick: hardware test

A Raspberry Pi Zero 2 W plugged into the machine's USB port, where the USB stick goes. The machine
sees an ordinary stick; designs are sent to it over Wi-Fi. This first version is for testing one
machine; the shop server will drive it later.

## What you need
- Pi Zero 2 W kit (microSD card, card reader, power supply, case)
- micro-USB (male) to USB-A (male) **data** cable
- your Mac, on the same Wi-Fi as the machine

## The easy way: from the Library
In the Library, **Machines › Prepare a design stick**: give the stick a name (the next free one is
suggested) and the Wi-Fi, and download the card files. Then write **Raspberry Pi OS Lite (64-bit)**
to the card with Raspberry Pi Imager (answer **No** to customising the settings), take the card out
and back in, and copy everything in the zip onto the **bootfs** drive. On its first start (3 to 5
minutes, it restarts once) the Pi joins the Wi-Fi, takes its name, lets the Library in with its
key, installs the stick (`first-start.sh`, then `setup.sh`) and takes the Wi-Fi password off the
card. The Library's window waits for it, then makes or links its machine. There is no password: the
stick only lets in the Library's key and the keys given under "More settings".
If it never answers, `wifi-report.txt` on the card's bootfs shows what the Pi saw.

The sections below are the manual way, kept for testing.

## 1. Prepare the card (on the Mac)
1. Install **Raspberry Pi Imager** from raspberrypi.com/software and open it.
2. Device: **Raspberry Pi Zero 2 W**. OS: **Raspberry Pi OS (other) › Raspberry Pi OS Lite (64-bit)**.
   Storage: the microSD card (in the card reader).
3. When it offers to customise the settings, choose **Edit settings** (or go through the
   customisation pages) and set:
   - hostname: `ecstick1` (`ecstick2`, `ecstick3`… for more machines)
   - username and password: e.g. `shop` and a password you keep
   - Wi-Fi: the shop network name and password (2.4 GHz), country **CA**
   - time zone: yours
   - **Services: Enable SSH**, with password authentication
   - leave **USB gadget mode off** if Imager offers it: ecstick sets up the USB port itself
4. Write the card. When Imager has finished, take the card out and put it back in: a disk named
   **bootfs** appears in Finder.
5. Copy this whole **pi-stick** folder onto **bootfs**. Eject the card.

**On Linux instead**, one command does steps 1–5 and the setup of step 2 (it asks for the Wi-Fi
and a password, and the Pi installs the stick by itself on its first start, then restarts):
```
sudo bash make-card.sh /dev/sdX 2026-10-06-raspios-trixie-arm64-lite.img.xz ecstick1
```
`lsblk` shows which `/dev/sdX` is the card. After 3 to 5 minutes, go straight to section 3.

To change the Wi-Fi of a card later, put it in the Linux PC and run `sudo bash set-wifi.sh`. The Pi
then also writes `wifi-report.txt` on the card at each start: the networks it sees and why it could
not connect. The Pi Zero 2 W only uses **2.4 GHz** networks with WPA2 (or WPA2/WPA3).

## 2. First start (at your desk)
1. Put the card in the Pi. Power it from the power supply in the port marked **PWR**.
   Wait 2 minutes (the first start is slow).
2. In Terminal on the Mac:
   ```
   ssh shop@ecstick1.local
   ```
   (`yes` to the fingerprint question, then the password.)
3. On the Pi:
   ```
   sudo bash /boot/firmware/pi-stick/setup.sh
   sudo reboot
   ```
4. Unplug the power supply.

## 3. At the machine
1. Turn the machine on. Plug the data cable into the Pi's port marked **USB** (the middle one, not
   PWR) and into the machine's USB port. The machine powers the Pi; it takes about 30 seconds to
   start.
2. From the Mac, check it:
   ```
   ssh shop@ecstick1.local ecstick status
   ```
   `Machine: configured` means the machine is using the stick.
3. Send designs from the Mac (in the unzipped pi-stick folder):
   ```
   bash send.sh shop@ecstick1.local trimtest.dst
   ```
   (`trimtest.dst`, the trim test pattern, is in this folder.)
   This replaces what is on the stick. `bash send.sh --add …` keeps the others.
   From a **Windows** PC: the `windows` folder (zipped as `EmbroideryCraft-Stick-Windows.zip`), drag
   designs onto `Send to stick.bat`; see its README.txt.

The Pi's green light: **on** = designs on the stick, **off** = empty, **blinking** = changing.

## Tested so far
2026-11-09, Tajima TFMX-C1501 (shop): the machine reads the stick and loads designs, accented and
long names show correctly (FAT16, names kept). New designs show up after leaving and re-entering
the USB menu, once the stick stays unplugged long enough: 1 second was too short, 5 seconds works
(`AWAY=5`, the default).
2026-11-10, same machine: designs sent from the Library (Machines › design stick) load with the
needles chosen in the Library. The machine takes them from the `.ct0` file sent next to each
`.tbf`, not from the TBF itself. The same day, starting the work order in Odoo's Shop Floor
(Atelier) on the Tajima 1 work center made the Library load its design on the stick by itself.
Not tested yet: powered from the machine's port alone, the TUMX, power
cycling the machine with the Pi plugged in.

## What to test, and note down
| # | Test | Result |
|---|------|--------|
| 1 | The Pi starts from the machine's USB port alone (the green light comes on, `ecstick status` answers) | |
| 2 | The machine sees the stick and lists the designs | |
| 3 | A design loads and sews correctly (trimtest.dst) | |
| 4 | Send a different design: the machine shows it (after you go back into its USB menu) | |
| 5 | Same as 4 while the machine shows the USB list: does it notice, freeze, or need a menu change? | |
| 6 | Turn the machine off and on with the Pi plugged in: the stick comes back | |
| 7 | Wi-Fi reaches the machine (`ssh` works from the Mac while the Pi is at the machine) | |
| 8 | Long file names (`Karate Club jacket.dst`) show correctly | |

Note the machine model and its USB menu path, and photos of anything odd.

The stick copies the shop's 2 GB sticks: 1 GB, FAT16, named DESIGNS. To see how one of your own
sticks is formatted, plug it into the Mac and run `diskutil info /Volumes/<its name> | grep -i personality`.

### If something fails
- **Test 1 fails** (the Pi does not start, or restarts): the machine's port gives too little power.
  Power the Pi from its supply in **PWR** as well, keeping the data cable in **USB**.
- **The machine does not see the stick**: on the Pi, set the format of one of your own sticks in
  `/etc/ecstick.conf` (`sudo nano /etc/ecstick.conf`): `FS=fat32`, or `SIZE_MB=512`, then
  `sudo ecstick format` and send again.
- **Names are cut or garbled**: set `NAMES=short` (8.3 names like `KARATE_C.DST`).
- **The machine does not notice new designs**: try `SWAP=eject` instead of `replug`.
- **`ecstick1.local` is not found**: the Pi is not on the Wi-Fi; check the name and password
  given to Imager (2.4 GHz network), or connect a screen with the mini-HDMI adapter.

## Commands on the Pi
```
ecstick status        connection, stick and designs
ecstick load FILES    replace the designs
ecstick add FILES     add designs
ecstick remove NAMES  take designs off the stick
ecstick clear         empty the stick
ecstick replug        unplug and plug the stick back
ecstick format        new empty stick from the settings in /etc/ecstick.conf
```
Every change unplugs the stick from the machine for a second, changes it, and plugs it back: the
machine never reads a stick that is being written. Do not send while the machine is reading a design.
