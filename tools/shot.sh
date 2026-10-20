#!/bin/zsh
# shot.sh <out.png> : captures the Mosamem window (works even when other windows cover it).
dir="${0:A:h}"
id=$("$dir/winid" mosamem) || { echo "Mosamem window not found" >&2; exit 1; }
screencapture -x -o -l "$id" "$1"
