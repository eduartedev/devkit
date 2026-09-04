@echo off
set DEVKIT_ROOT=C:\DevKit
if exist "%~dp0target\release\devkit.exe" (
  "%~dp0target\release\devkit.exe" %*
) else (
  "%~dp0target\debug\devkit.exe" %*
)
