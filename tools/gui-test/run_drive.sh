#!/bin/bash
export DISPLAY=:99
Xvfb :99 -screen 0 1500x950x24 >/dev/null 2>&1 &
XPID=$!
sleep 1
cd /tmp/exp && /home/claude/embroiderycraft/target/debug/embcraft-gui --size 1360x860 >/tmp/gui_run.log 2>&1 &
GPID=$!
python3 -I /tmp/claude-0/-home-claude/7064b347-7784-5d50-ad62-bc3b9a52f30c/scratchpad/drive.py /tmp/drv "$1"
kill $GPID $XPID 2>/dev/null
