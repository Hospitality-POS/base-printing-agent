; PrintAgent Helper — NSIS installer script
; Produces a classic setup wizard that works on Windows 7/8/8.1/10/11.
;
; Build:
;   1. cargo build --release -p print-agent-helper
;   2. npm run build, then copy dist\ to installer\web\
;   3. Copy target\release\print-agent-helper.exe to this folder
;   4. makensis print-agent-helper.nsi  (winget install NSIS.NSIS)

!include "MUI2.nsh"

!ifndef HELPER_EXE
  !define HELPER_EXE "print-agent-helper.exe"
!endif
!ifndef OUTFILE
  !define OUTFILE "PrintAgentHelper-Setup.exe"
!endif
!ifndef APP_VERSION
  !define APP_VERSION "0.10.0"
!endif

Name "PrintAgent"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\PrintAgent"
RequestExecutionLevel user          ; no admin needed — per-user install
SetCompressor /SOLID lzma

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS    ; "how to run it" options live here
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Section "PrintAgent Helper (required)" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"
  File "${HELPER_EXE}"
  File "..\src-tauri\icons\icon.ico"

  FileOpen $0 "$INSTDIR\PrintAgent.cmd" w
  FileWrite $0 '@echo off$\r$\n'
  FileWrite $0 'cd /d "%~dp0"$\r$\n'
  FileWrite $0 'print-agent-helper.exe --open$\r$\n'
  FileWrite $0 'if errorlevel 1 pause$\r$\n'
  FileClose $0

  SetOutPath "$INSTDIR\web"
  File /r "web\*.*"

  WriteUninstaller "$INSTDIR\Uninstall.exe"

  CreateDirectory "$SMPROGRAMS\PrintAgent"
  CreateShortcut "$SMPROGRAMS\PrintAgent\PrintAgent.lnk" \
    "$INSTDIR\PrintAgent.cmd" "" \
    "$INSTDIR\icon.ico" 0
  CreateShortcut "$SMPROGRAMS\PrintAgent\Uninstall PrintAgent.lnk" \
    "$INSTDIR\Uninstall.exe"

  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\PrintAgent" "DisplayName" "PrintAgent"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\PrintAgent" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\PrintAgent" "Publisher" "Basepoint Cloud Inc"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\PrintAgent" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\PrintAgent" "DisplayIcon" "$INSTDIR\icon.ico"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\PrintAgent" "UninstallString" "$INSTDIR\Uninstall.exe"
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\PrintAgent" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\PrintAgent" "NoRepair" 1

  ExecShell "open" "$INSTDIR\PrintAgent.cmd"
SectionEnd

Section "Start automatically when I sign in" SecAutostart
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" \
    "PrintAgentHelper" '"$INSTDIR\print-agent-helper.exe"'
SectionEnd

Section "Desktop shortcut" SecShortcut
  CreateShortcut "$DESKTOP\PrintAgent.lnk" \
    "$INSTDIR\PrintAgent.cmd" "" \
    "$INSTDIR\icon.ico" 0
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecMain} "The helper service and web UI (required)."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAutostart} "Recommended for POS machines — PrintAgent starts with Windows."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecShortcut} "Shortcut that opens the PrintAgent page in your browser."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "Uninstall"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "PrintAgentHelper"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\PrintAgent"
  Delete "$DESKTOP\PrintAgent.lnk"
  Delete "$SMPROGRAMS\PrintAgent\PrintAgent.lnk"
  Delete "$SMPROGRAMS\PrintAgent\Uninstall PrintAgent.lnk"
  RMDir "$SMPROGRAMS\PrintAgent"
  RMDir /r "$INSTDIR"
SectionEnd
