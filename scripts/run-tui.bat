@echo off
REM Build (release) and launch the terminal UI on Windows.
cargo build --release --bin ff
if errorlevel 1 exit /b 1
target\release\ff.exe %*
