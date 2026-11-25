#!/usr/bin/env python3
"""Write the Wi-Fi profile straight into a Pi card's NetworkManager folder:
    SSID=... WIFIPW=... python3 nm-wifi.py ROOTFS
Pi OS's cloud-init route (network-config → netplan) can leave an empty profile, and then the Pi
never joins the Wi-Fi; a NetworkManager keyfile does not depend on it."""
import os
import sys
import uuid


def value(text):
    # keyfile values: a backslash escapes, and leading or trailing spaces must be escaped
    text = text.replace("\\", "\\\\")
    if text.startswith(" "):
        text = "\\s" + text[1:]
    if text.endswith(" "):
        text = text[:-1] + "\\s"
    return text


def main():
    root = sys.argv[1]
    ssid, psk = os.environ["SSID"], os.environ["WIFIPW"]
    if ";" in ssid or "\n" in ssid:
        ssid_line = ";".join(str(b) for b in ssid.encode()) + ";"
    else:
        ssid_line = value(ssid)
    profile = f"""[connection]
id=shop-wifi
uuid={uuid.uuid4()}
type=wifi
interface-name=wlan0
autoconnect=true

[wifi]
mode=infrastructure
ssid={ssid_line}

[wifi-security]
key-mgmt=wpa-psk
psk={value(psk)}

[ipv4]
method=auto

[ipv6]
method=auto
"""
    folder = os.path.join(root, "etc/NetworkManager/system-connections")
    os.makedirs(folder, exist_ok=True)
    path = os.path.join(folder, "shop-wifi.nmconnection")
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as f:
        f.write(profile)
    os.chmod(path, 0o600)
    os.chown(path, 0, 0)


main()
