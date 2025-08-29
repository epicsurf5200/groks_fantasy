@echo off

echo Running main.py...
if exist main.py (
    python main.py
) else (
    echo Error: main.py not found in current directory.
    exit /b 1
)

echo Execution complete.