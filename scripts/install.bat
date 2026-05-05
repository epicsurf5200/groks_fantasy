@echo off
REM install.bat - Windows installer.
REM Installs Rust (if missing) and builds release binaries ff.exe + ff-gui.exe
REM into target\release\.

setlocal

where cargo >nul 2>nul
if errorlevel 1 (
    echo Rust toolchain not found.
    echo Please install rustup from https://rustup.rs and re-run this script.
    exit /b 1
)

echo Building release binaries...
cargo build --release --bins
if errorlevel 1 (
    echo Build failed.
    exit /b 1
)

echo.
echo Built:
dir /b target\release\ff.exe target\release\ff-gui.exe

echo.
echo Next steps:
echo   target\release\ff.exe init sleeper        REM or: espn / yahoo
echo   notepad config.yaml
echo   set ANTHROPIC_API_KEY=sk-ant-...
echo   target\release\ff-gui.exe                 REM desktop GUI
echo   target\release\ff.exe                     REM terminal UI

endlocal
