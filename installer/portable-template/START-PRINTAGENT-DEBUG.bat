@echo off
cd /d "%~dp0"
echo Starting PrintAgent Helper in debug mode.
echo Keep this window open while using PrintAgent.
echo.
print-agent-helper.exe
echo.
echo PrintAgent Helper exited.
pause
