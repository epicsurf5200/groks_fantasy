#!/bin/bash
set -e
VENV_DIR="venv_groks_fantasy"
if [ ! -d "$VENV_DIR" ]; then
    echo "Creating virtual environment..."
    python3 -m venv $VENV_DIR
else
    echo "Virtual environment already exists."
fi
source $VENV_DIR/bin/activate
echo "Installing dependencies from packages folder..."
if [ -d "packages" ] && [ "$(ls -A packages/*.whl 2>/dev/null)" ]; then
    pip install packages/*.whl
else
    echo "Error: No .whl files found in packages folder."
    exit 1
fi
echo "Installed packages:"
pip list
echo "Setup complete. Virtual environment is activated."