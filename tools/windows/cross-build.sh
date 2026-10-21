#!/bin/zsh
# Cross-builds Mosamem for Windows x64 on a Mac with cargo-xwin (clang-cl + lld-link and the
# Windows SDK that cargo-xwin downloads on first use, after you accept Microsoft's license).
#   tools/windows/cross-build.sh            the installer's input (stops at makensis; then run
#                                           tools/windows/make-installer-vm.sh x64)
#   tools/windows/cross-build.sh harness    mosamem.exe with the test harness, no installer
# Needs: brew install llvm lld; cargo install cargo-xwin; rustup target add x86_64-pc-windows-msvc
set -e
cd "${0:A:h}/../.."
llvm=$(brew --prefix llvm)
lld=$(brew --prefix lld)
# libwebp's x86 code asks for SSE4.1 intrinsics, which clang-cl only allows with -msse4.1 (MSVC
# allows them always). Every x64 CPU since 2008 has SSE4.1, and Windows 11 requires it. A wrapper,
# because the cc crate replaces CFLAGS for some builds.
wrap=$(mktemp -d)
printf '#!/bin/sh\nexec %s/bin/clang-cl -msse4.1 "$@"\n' "$llvm" > $wrap/clang-cl
chmod +x $wrap/clang-cl
export PATH="$wrap:$llvm/bin:$lld/bin:$HOME/.cargo/bin:$PATH"
unset XWIN_CROSS_COMPILER
if [[ $1 == harness ]]; then
  VITE_HARNESS=1 npx tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --features harness --no-bundle
  ls -la target/x86_64-pc-windows-msvc/release/mosamem.exe
else
  # tauri's bundler stops at makensis (Homebrew's crashes on macOS 27): that's expected
  npx tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc || true
  [[ -f target/x86_64-pc-windows-msvc/release/nsis/x64/installer.nsi ]] || { echo "the build failed before the installer step" >&2; exit 1; }
  echo "ready: tools/windows/make-installer-vm.sh x64"
fi
rm -rf $wrap
