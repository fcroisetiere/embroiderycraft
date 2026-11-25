@echo off
rem Adds "Design stick" to the right-click Send to menu.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0stick.ps1" sendto
pause
