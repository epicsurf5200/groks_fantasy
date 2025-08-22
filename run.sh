#!/bin/bash

# Exit on any error
set -e

# Run install.sh to set up virtual environment and dependencies
echo "Running install.sh..."
if [ -f "./install.sh" ]; then
    chmod +x ./install.sh
    ./install.sh
else
    echo "Error: install.sh not found in current directory."
    exit 1
fi

# Activate virtual environment
VENV_DIR="venv_groks_fantasy"
if [ -d "$VENV_DIR" ]; then
    echo "Activating virtual environment..."
    source $VENV_DIR/bin/activate
else
    echo "Error: Virtual environment directory ($VENV_DIR) not found."
    exit 1
fi

# Run main.py
echo "Running main.py..."
if [ -f "main.py" ]; then
    python3 main.py
else
    echo "Error: main.py not found in current directory."
    exit 1
fi

echo "Execution complete."