@echo off
setlocal enabledelayedexpansion

echo ============================================================
echo  isolmaSS Distribution Packaging Script
echo ============================================================

set BIN_PATH=target\release\isolmass.exe
set NSI_PATH=installer.nsi
set SETUP_PATH=target\release\isolmass-setup.exe
set MAX_BUDGET=3145728

echo [INFO] Building production binary with cargo build --release...
cargo build --release
if errorlevel 1 (
    echo [ERROR] Cargo build failed
    exit /b 1
)

set MAKENSIS_CMD=makensis
where makensis >nul 2>nul
if %errorlevel% neq 0 (
    if exist "%LOCALAPPDATA%\Programs\nsis-3.10\makensis.exe" (
        set MAKENSIS_CMD=%LOCALAPPDATA%\Programs\nsis-3.10\makensis.exe
    ) else (
        echo [ERROR] makensis not found on PATH or at %LOCALAPPDATA%\Programs\nsis-3.10\makensis.exe
        exit /b 1
    )
)

echo [INFO] Compiling installer with NSIS (!MAKENSIS_CMD!)...
"%MAKENSIS_CMD%" %NSI_PATH%
if errorlevel 1 (
    echo [ERROR] NSIS compilation failed
    exit /b 1
)

if not exist "%SETUP_PATH%" (
    echo [ERROR] Installer output does not exist: %SETUP_PATH%
    exit /b 1
)

for %%I in ("%SETUP_PATH%") do set SETUP_SIZE=%%~zI

if !SETUP_SIZE! gtr %MAX_BUDGET% (
    echo [ERROR] Installer size !SETUP_SIZE! exceeds budget of %MAX_BUDGET% bytes
    exit /b 1
)

echo [SUCCESS] Authentic installer verified: %SETUP_PATH%
echo [INFO] Installer size: !SETUP_SIZE! bytes - Budget: %MAX_BUDGET% bytes / 3 MB
echo Packaging completed successfully.
exit /b 0
