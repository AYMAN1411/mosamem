; Mosamem installer hooks (tauri NSIS). ONNX Runtime needs the Microsoft Visual C++ 2015–2022
; runtime. Many PCs have it; when it's missing, say so and open Microsoft's download page (installing
; it needs an administrator, and this installer runs without one).
!macro NSIS_HOOK_POSTINSTALL
  SetRegView 64
  ReadRegDWORD $0 HKLM "SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64" "Installed"
  ${If} $0 != 1
  ${AndIfNot} ${Silent}
    MessageBox MB_OK|MB_ICONINFORMATION "مصمم يحتاج «Microsoft Visual C++ Runtime». ستُفتح صفحة تنزيلها من Microsoft: ثبّتها ثم افتح مصمم.$\r$\n$\r$\nMosamem needs the Microsoft Visual C++ Runtime. Microsoft's download page will open: install it, then open Mosamem."
    ExecShell "open" "https://aka.ms/vs/17/release/vc_redist.x64.exe"
  ${EndIf}
  SetRegView lastused
!macroend
