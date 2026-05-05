@echo off
REM Build (release) and launch the desktop GUI on Windows.
cargo build --release --bin ff-gui
if errorlevel 1 exit /b 1
target\release\ff-gui.exe %*
