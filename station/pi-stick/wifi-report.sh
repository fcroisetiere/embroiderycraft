#!/bin/bash
# Writes the Pi's Wi-Fi state to the boot partition (readable on any PC), 90 s after start.
sleep 90
# the stick works without it; this is only for checking the Wi-Fi
{
  date; uptime
  echo "== rfkill"; rfkill list
  echo "== devices"; nmcli -f DEVICE,TYPE,STATE,CONNECTION device
  echo "== profiles"; nmcli -f NAME,TYPE,AUTOCONNECT,DEVICE connection
  echo "== networks seen"; nmcli -f SSID,CHAN,FREQ,SIGNAL,SECURITY device wifi list --rescan yes
  echo "== addresses"; ip -4 addr
  echo "== regulatory"; iw reg get 2>&1 | head -5
  echo "== NetworkManager log"; journalctl -b -u NetworkManager --no-pager | tail -80
  echo "== wpa_supplicant log"; journalctl -b -u wpa_supplicant --no-pager | tail -40
  echo "== kernel wifi"; dmesg | grep -i -E "brcm|wlan|cfg80211" | tail -30
} > /boot/firmware/wifi-report.txt 2>&1
sync
