@echo off
cd /d "%~dp0"
if not exist print-agent-helper.exe (
  echo print-agent-helper.exe is missing from this folder.
  pause
  exit /b 1
)

echo Starting PrintAgent Helper...
start "" "%~dp0print-agent-helper.exe"

echo Waiting for the helper to listen on port 14200...
ping 127.0.0.1 -n 4 >nul

set "URL=http://127.0.0.1:14200"
if exist "%ProgramFiles%\Google\Chrome\Application\chrome.exe" (
  start "" "%ProgramFiles%\Google\Chrome\Application\chrome.exe" "%URL%"
) else if exist "%ProgramFiles(x86)%\Google\Chrome\Application\chrome.exe" (
  start "" "%ProgramFiles(x86)%\Google\Chrome\Application\chrome.exe" "%URL%"
) else (
  start "" "%URL%"
)
echo.
echo If the setup page does not open, open this address in Chrome:
echo   %URL%
echo.
