@echo off
setlocal
set "SOURCE=%~dp0"
set "TARGET=%LOCALAPPDATA%\PrintAgent"
set "STARTMENU=%APPDATA%\Microsoft\Windows\Start Menu\Programs\PrintAgent"

echo Installing PrintAgent to %TARGET%
if not exist "%TARGET%" mkdir "%TARGET%"
if not exist "%TARGET%\web" mkdir "%TARGET%\web"
if not exist "%STARTMENU%" mkdir "%STARTMENU%"

xcopy /E /I /Y "%SOURCE%web" "%TARGET%\web\" >nul
copy /Y "%SOURCE%print-agent-helper.exe" "%TARGET%\" >nul
copy /Y "%SOURCE%START-PRINTAGENT.bat" "%TARGET%\" >nul
copy /Y "%SOURCE%OPEN-SETUP.bat" "%TARGET%\" >nul
copy /Y "%SOURCE%icon.ico" "%TARGET%\" >nul

reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v PrintAgent /t REG_SZ /d "\"%TARGET%print-agent-helper.exe\"" /f >nul

cscript //nologo "%SOURCE%CreateShortcut.vbs" "%USERPROFILE%\Desktop\PrintAgent.lnk" "%TARGET%\START-PRINTAGENT.bat" "%TARGET%" "%TARGET%\icon.ico"
cscript //nologo "%SOURCE%CreateShortcut.vbs" "%STARTMENU%\PrintAgent.lnk" "%TARGET%\START-PRINTAGENT.bat" "%TARGET%" "%TARGET%\icon.ico"

echo PrintAgent installed.
start "" "%TARGET%\START-PRINTAGENT.bat"
