@echo off
setlocal enabledelayedexpansion

set BIN_PATH=target\release\isolmass.exe
set NSI_PATH=installer.nsi
set SETUP_PATH=target\release\isolmass-setup.exe
set DIGEST_PATH=target\release\isolmass-setup.exe.sha256
set MAX_BUDGET=3145728

for /f "usebackq delims=" %%V in (`powershell -NoProfile -Command "$metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json; ($metadata.packages | Where-Object name -eq 'isolmass').version"`) do set PRODUCT_VERSION=%%V
if not defined PRODUCT_VERSION (
    echo [ERROR] Could not read the package version from Cargo metadata
    exit /b 1
)

echo ============================================================
echo  isolmaSS %PRODUCT_VERSION% Distribution Packaging
echo ============================================================

echo [INFO] Building production binary...
cargo build --release
if errorlevel 1 exit /b 1

call :sign "%BIN_PATH%"
if errorlevel 1 exit /b 1

set MAKENSIS_CMD=makensis
where makensis >nul 2>nul
if errorlevel 1 (
    if exist "%LOCALAPPDATA%\Programs\nsis-3.10\makensis.exe" (
        set MAKENSIS_CMD=%LOCALAPPDATA%\Programs\nsis-3.10\makensis.exe
    ) else (
        echo [ERROR] makensis was not found
        exit /b 1
    )
)

echo [INFO] Compiling NSIS installer...
"!MAKENSIS_CMD!" /DPRODUCT_VERSION=%PRODUCT_VERSION% "%NSI_PATH%"
if errorlevel 1 exit /b 1
if not exist "%SETUP_PATH%" (
    echo [ERROR] Installer output does not exist: %SETUP_PATH%
    exit /b 1
)

call :sign "%SETUP_PATH%"
if errorlevel 1 exit /b 1

for %%I in ("%SETUP_PATH%") do set SETUP_SIZE=%%~zI
if !SETUP_SIZE! gtr %MAX_BUDGET% (
    echo [ERROR] Installer size !SETUP_SIZE! exceeds %MAX_BUDGET% bytes
    exit /b 1
)

powershell -NoProfile -Command "$hash = (Get-FileHash -Algorithm SHA256 '%SETUP_PATH%').Hash.ToLowerInvariant(); Set-Content -Encoding ascii '%DIGEST_PATH%' ($hash + '  isolmass-setup.exe')"
if errorlevel 1 exit /b 1

echo [SUCCESS] Installer: %SETUP_PATH%
echo [INFO] Version: %PRODUCT_VERSION%
echo [INFO] Size: !SETUP_SIZE! bytes
for /f "usebackq tokens=1" %%H in ("%DIGEST_PATH%") do echo [INFO] SHA-256: %%H
exit /b 0

:sign
set FILE_TO_SIGN=%~1
if defined SIGNTOOL_CERT_SHA1 (
    where signtool >nul 2>nul
    if errorlevel 1 (
        echo [ERROR] SIGNTOOL_CERT_SHA1 is set but signtool was not found
        exit /b 1
    )
    echo [INFO] Signing %FILE_TO_SIGN%...
    signtool sign /sha1 "%SIGNTOOL_CERT_SHA1%" /fd SHA256 /tr "http://timestamp.digicert.com" /td SHA256 "%FILE_TO_SIGN%"
    if errorlevel 1 exit /b 1
    signtool verify /pa /all "%FILE_TO_SIGN%"
    if errorlevel 1 exit /b 1
) else (
    if /I "%REQUIRE_SIGNING%"=="1" (
        echo [ERROR] Production release requires SIGNTOOL_CERT_SHA1
        exit /b 1
    )
    echo [WARN] %FILE_TO_SIGN% is unsigned. Set SIGNTOOL_CERT_SHA1 for a trusted release.
)
exit /b 0
