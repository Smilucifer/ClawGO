@echo off
REM Build the embedded Python environment for ClawGO.
REM Run this once on the development machine to prepare python-runtime/ for packaging.
REM
REM Prerequisites: Internet connection (to download Python and pip packages).
REM
REM What it does:
REM   1. Downloads python-build-standalone (Python 3.13 for Windows x64)
REM   2. Extracts to src-tauri/python-runtime/python/
REM   3. Installs yfinance + orjson via pip
REM   4. Verifies the installation

setlocal enabledelayedexpansion

set PYTHON_VERSION=3.13.13
set RELEASE_TAG=20260510
set TARGET_DIR=%~dp0..\src-tauri\python-runtime
set PYTHON_DIR=%TARGET_DIR%\python
set DOWNLOAD_URL=https://github.com/astral-sh/python-build-standalone/releases/download/%RELEASE_TAG%/cpython-%PYTHON_VERSION%+%RELEASE_TAG%-x86_64-pc-windows-msvc-install_only.tar.gz
set DOWNLOAD_FILE=%TEMP%\python-build-standalone.tar.gz

echo [1/4] Downloading Python %PYTHON_VERSION%...
if exist "%PYTHON_DIR%\python.exe" (
    echo   Python already exists at %PYTHON_DIR%\python.exe, skipping download.
    goto :install_deps
)

if not exist "%TARGET_DIR%" mkdir "%TARGET_DIR%"
echo   Downloading from: %DOWNLOAD_URL%
curl -L -o "%DOWNLOAD_FILE%" "%DOWNLOAD_URL%"
if errorlevel 1 (
    echo ERROR: Download failed.
    exit /b 1
)

echo [2/4] Extracting Python...
tar xzf "%DOWNLOAD_FILE%" -C "%TARGET_DIR%"
if errorlevel 1 (
    echo ERROR: Extraction failed.
    exit /b 1
)
del "%DOWNLOAD_FILE%"

if not exist "%PYTHON_DIR%\python.exe" (
    echo ERROR: python.exe not found after extraction.
    exit /b 1
)
echo   Python extracted to %PYTHON_DIR%

:install_deps
echo [3/4] Installing Python dependencies...
"%PYTHON_DIR%\python.exe" -m pip install --no-warn-script-location yfinance orjson akshare xtquant playwright patchright py_mini_racer scrapling
if errorlevel 1 (
    echo ERROR: pip install failed.
    exit /b 1
)

echo [4/4] Verifying installation...
"%PYTHON_DIR%\python.exe" -c "import yfinance; print('yfinance', yfinance.__version__); import orjson; print('orjson', orjson.__version__); import akshare; print('akshare', akshare.__version__)"
if errorlevel 1 (
    echo ERROR: Verification failed.
    exit /b 1
)

echo.
echo === Python environment ready ===
echo Location: %PYTHON_DIR%
echo.
echo The python-runtime/ directory is now ready for packaging with Tauri.
echo Run 'npm run tauri build' to include it in the installer.
