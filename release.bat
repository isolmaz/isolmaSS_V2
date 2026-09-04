@echo off
setlocal
if not defined SIGNTOOL_CERT_SHA1 (
    echo [ERROR] Set SIGNTOOL_CERT_SHA1 to the SHA-1 thumbprint of the release code-signing certificate.
    exit /b 1
)
set REQUIRE_SIGNING=1
call package.bat
exit /b %errorlevel%
