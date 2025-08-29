@echo off
set VENV_DIR=venv_groks_fantasy

if not exist %VENV_DIR% (
    echo Creating virtual environment...
    python -m venv %VENV_DIR%
) else (
    echo Virtual environment already exists.
)

echo Activating virtual environment...
call %VENV_DIR%\Scripts\activate

set PKG_DIR=packages\windows

echo Installing dependencies from %PKG_DIR%...
set FOUND=0
for %%f in (%PKG_DIR%\*.whl) do (
    set FOUND=1
    pip install "%%f"
)

if %FOUND% == 0 (
    echo Error: No .whl files found in %PKG_DIR%.
    exit /b 1
)

echo Installed packages:
pip list

echo Setup complete. Virtual environment is activated.