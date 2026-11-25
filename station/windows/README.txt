EmbroideryCraft Shop Library for Windows (beta)
================================================

The Shop Library keeps the shop's designs in one place: previews, sizes, stitch counts, thread
colours, colourways, customers, and the machines with their design sticks. Everyone opens it in a
web browser.

Start it
--------
Double-click "Start Shop Library.bat". The library opens in the browser at http://localhost:8080/.
The first time, create the administrator account. Closing the black window stops the library.

Windows may warn that the program is from an unknown publisher (it is not signed). Click
"More info", then "Run anyway".

The designs are kept in %APPDATA%\EmbroideryCraft Library. Use Settings > Backups in the library
to copy them somewhere safe every day.

Open it from the other computers in the shop
--------------------------------------------
As it comes, only this computer can open the library. To let the others in, edit
"Start Shop Library.bat" (right-click > Edit) and change 127.0.0.1:8080 to 0.0.0.0:8080. Windows
asks whether to allow it on the network: allow it on private networks only. The others then open
http://<this computer's name>:8080/ . This computer must stay on while people use the library.

For a library that is always on, a small Linux computer or virtual machine is better: use the
Linux kit (EmbroideryCraft-Shop-Library-<version>-linux-x64.tar.gz) and its README.

The library speaks plain HTTP. That is fine inside the shop, but never open its port to the
internet.

Beta
----
This is a preview. The design stick (a Raspberry Pi Zero 2 W in the machine's USB port) has been
tested on one Tajima TFMX-C1501. TBF files are made without an official specification. Please
report what works and what doesn't at https://embroiderycraft.org/report.html

Licence: MIT or Apache-2.0 (LICENSE-MIT, LICENSE-APACHE).
