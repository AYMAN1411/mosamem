#!/bin/zsh
# Downloads the public PSD test files of psd-tools (MIT license, © Kota Yamaguchi) for the PSD
# round-trip report. They are test data only: not part of Mosamem, and not committed.
set -e
cd "${0:A:h}/.."
dest=tests/psd-fixtures/psd-tools
mkdir -p $dest
curl -sf https://raw.githubusercontent.com/psd-tools/psd-tools/main/LICENSE -o $dest/LICENSE
curl -sf 'https://api.github.com/repos/psd-tools/psd-tools/contents/tests/psd_files' |
  python3 -c "import json,sys; [print(x['name']) for x in json.load(sys.stdin) if x['name'].endswith('.psd')]" |
  while read name; do
    [[ -f $dest/$name ]] || curl -sf "https://raw.githubusercontent.com/psd-tools/psd-tools/main/tests/psd_files/$name" -o "$dest/$name"
  done
ls $dest/*.psd | wc -l
