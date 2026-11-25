#!/bin/bash
# From the Mac: send designs to a stick, replacing what is on it (or adding, with --add).
#   bash send.sh ecstick1.local design.dst [more.dst ...]
#   bash send.sh --add ecstick1.local design.dst
set -euo pipefail
cmd=load
[ "${1:-}" = --add ] && { cmd=add; shift; }
[ $# -ge 2 ] || { sed -n '2,4p' "$0"; exit 2; }
host=$1
shift
for f in "$@"; do [ -f "$f" ] || { echo "no file $f" >&2; exit 1; }; done
# one connection: pack the files, unpack on the Pi, load them; COPYFILE_DISABLE keeps the Mac's
# hidden ._ files out
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
for f in "$@"; do cp "$f" "$tmp/"; done
COPYFILE_DISABLE=1 tar -c -C "$tmp" . | ssh "$host" \
    'd=$(mktemp -d) && tar -x -C "$d" && sudo ecstick '"$cmd"' "$d"/* ; s=$?; rm -rf "$d"; exit $s'
