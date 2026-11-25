DESIGN STICK - sending designs from Windows
===========================================

Unzip this folder anywhere (for example on the Desktop). Nothing to install:
Windows 10 and 11 already have what it needs.

FIRST TIME
  Double-click "Stick status.bat".
  It asks once for the Pi's password (it does not show while you type).
  After that, no password is needed from this PC.
  "Machine: configured" means the machine is using the stick.

SENDING DESIGNS
  Drag design files onto "Send to stick.bat"  -> they REPLACE what is on the stick.
  Drag design files onto "Add to stick.bat"   -> they are ADDED to what is on the stick.
  Then, on the machine, open the USB menu again to see them.

  Easier: double-click "Add to Send-to menu.bat" once. Then right-click designs
  > Send to > Design stick.  (On Windows 11: right-click > Show more options > Send to.)

ANOTHER STICK (second machine)
  Open stick.txt in Notepad and change ecstick1 to ecstick2 (keep "shop@").
  If the name is not found, put the Pi's address instead, e.g. shop@192.168.1.79

IF IT FAILS
  "The stick did not answer": the Pi is off, or not on this PC's network.
  Do not send while the machine is reading a design.
