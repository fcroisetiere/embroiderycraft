@echo off
rem Drag design files onto this: they replace what is on the stick.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0stick.ps1" load %*
pause
