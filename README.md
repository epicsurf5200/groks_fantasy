# Grok's Fantasy

ESPN fantasy football app that uses Grok to make decisions/suggestions.

## Setup
1. Clone the repo: `git clone https://github.com/epicsurf5200/groks_fantasy.git`
2. Populate `packages/` with dependencies (run on your OS):
   - macOS/Linux: `pip download --platform macosx_10_9_x86_64 --python-version 3.10 --only-binary=:all: -d packages requests==2.32.3 pyyaml==6.0.2 espn-api==0.35.0 pandas==2.2.2`
   - Windows: `pip download --platform win_amd64 --python-version 3.10 --only-binary=:all: -d packages requests==2.32.3 pyyaml==6.0.2 espn-api==0.35.0 pandas==2.2.2`
3. Configure `config.yaml` with ESPN and Grok API credentials (not committed—see .gitignore).

## Running the App
- macOS/Linux: `./run.sh` (or `bash run.sh`)
- Windows: `run.bat`

This executes the install script (sets up venv and deps) and runs `main.py`.