#!/bin/zsh
# shot.sh <out.png> : captures the Mosamem window (works even when other windows cover it).
dir="${0:A:h}"
# the dev or harness build by its process id (an installed Mosamem has the same name)
pid=$(pgrep -f 'target/(debug|release|harness)/.*mosamem$' | head -1)
id=$("$dir/winid" ${pid:-mosamem}) || { echo "Mosamem window not found" >&2; exit 1; }
screencapture -x -o -l "$id" "$1"
