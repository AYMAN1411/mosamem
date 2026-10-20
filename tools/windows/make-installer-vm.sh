#!/bin/zsh
# Makes the Windows installer from a Mac cross-build, using NSIS on a Windows machine reached over
# SSH (Homebrew's makensis crashes on macOS 27, even on an empty script).
#   npx tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc   (stops at makensis)
#   tools/windows/make-installer-vm.sh x64 [host]   → release/Mosamem-Setup-x64.exe
# The first run downloads portable NSIS 3.11 (zlib/libpng license) from SourceForge on that machine.
set -e
cd "${0:A:h}/../.."
arch=${1:-x64}; host=${2:-adawat-win}
triple=$([[ $arch == x64 ]] && echo x86_64-pc-windows-msvc || echo aarch64-pc-windows-msvc)
nsis=target/$triple/release/nsis/$arch
[[ -f $nsis/installer.nsi ]] || { echo "no $nsis/installer.nsi: run the tauri build first" >&2; exit 1; }
stage=$(mktemp -d)
# every absolute Mac path in the script becomes C:\Mosamem\build\<file name>
for p in $(grep -o '"/Users/[^"]*"' $nsis/installer.nsi | tr -d '"' | sort -u); do
  [[ -d $p ]] && cp -R $p $stage/plugins || cp $p $stage/
done
cp $nsis/*.nsh $stage/ 2>/dev/null || true
sed -E 's#"/Users/[^"]*/Plugins/x86-unicode/additional"#"C:\\Mosamem\\build\\plugins"#; s#"/Users/[^"]*/([^/"]+)"#"C:\\Mosamem\\build\\\1"#g' $nsis/installer.nsi > $stage/installer.nsi
ssh $host 'if (-not (Test-Path C:\Mosamem\nsis\makensis.exe)) {
  New-Item -ItemType Directory -Force C:\Mosamem | Out-Null
  Invoke-WebRequest -UseBasicParsing -UserAgent "Wget" "https://downloads.sourceforge.net/project/nsis/NSIS%203/3.11/nsis-3.11.zip" -OutFile C:\Mosamem\nsis.zip
  Expand-Archive -Force C:\Mosamem\nsis.zip C:\Mosamem
  Rename-Item C:\Mosamem\nsis-3.11 nsis
}
Remove-Item -Recurse -Force C:\Mosamem\build -ErrorAction SilentlyContinue; New-Item -ItemType Directory C:\Mosamem\build | Out-Null'
scp -q -r $stage/* $host:C:/Mosamem/build/
ssh $host 'cd C:\Mosamem\build; & C:\Mosamem\nsis\makensis.exe /V2 /INPUTCHARSET UTF8 installer.nsi; if ($LASTEXITCODE) { exit $LASTEXITCODE }; Get-ChildItem *.exe | Where-Object Name -ne mosamem.exe | Select -First 1 -ExpandProperty Name' > $stage/out.txt
out=$(tail -1 $stage/out.txt | tr -d '\r')
mkdir -p release
scp -q "$host:C:/Mosamem/build/$out" release/Mosamem-Setup-$arch.exe
shasum -a 256 release/Mosamem-Setup-$arch.exe
rm -rf $stage
