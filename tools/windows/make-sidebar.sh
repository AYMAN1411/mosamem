#!/bin/zsh
# The Windows installer's side picture: drawn by sidebar.swift at 2× (sharp on high-DPI screens;
# NSIS fits it to its frame), saved as the 24-bit BMP that NSIS takes.
set -e
cd "${0:A:h}/../.."
tmp=$(mktemp -d)
swift tools/windows/sidebar.swift $tmp/sidebar.png 2
python3 -c "from PIL import Image; Image.open('$tmp/sidebar.png').convert('RGB').save('src-tauri/windows/sidebar.bmp')"
rm -rf $tmp
ls -la src-tauri/windows/sidebar.bmp
