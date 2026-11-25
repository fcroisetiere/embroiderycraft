@echo off
rem Drag design files onto this: they are added to what is on the stick.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0stick.ps1" add %*
pause
