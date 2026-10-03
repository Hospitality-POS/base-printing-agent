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

Name "PrintAgent Helper"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\PrintAgent"
RequestExecutionLevel user          ; no admin needed — per-user install
SetCompressor /SOLID lzma

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS    ; "how to run it" options live here
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\print-agent-helper.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Start PrintAgent Helper now"
!define MUI_FINISHPAGE_LINK "Open PrintAgent setup in browser"
!define MUI_FINISHPAGE_LINK_LOCATION "http://localhost:14200"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Section "PrintAgent Helper (required)" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"
  File "${HELPER_EXE}"
  SetOutPath "$INSTDIR\web"
  File /r "web\*.*"

  WriteUninstaller "$INSTDIR\Uninstall.exe"
SectionEnd

Section "Start automatically when I sign in" SecAutostart
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" \
    "PrintAgentHelper" '"$INSTDIR\print-agent-helper.exe"'
SectionEnd

Section "Desktop shortcut" SecShortcut
  CreateShortcut "$DESKTOP\PrintAgent.lnk" "http://localhost:14200"
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecMain} "The helper service and web UI (required)."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAutostart} "Recommended for POS machines — PrintAgent starts with Windows."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecShortcut} "Shortcut that opens the PrintAgent page in your browser."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "Uninstall"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "PrintAgentHelper"
  Delete "$DESKTOP\PrintAgent.lnk"
  RMDir /r "$INSTDIR"
SectionEnd
