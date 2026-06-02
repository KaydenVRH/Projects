#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

echo "==> Setting up virtual environment..."
python3 -m venv .venv
source .venv/bin/activate

echo "==> Installing PyObjC (this may take a minute)..."
pip install PyObjC

echo "==> Creating symlink: /usr/local/bin/lwp"
sudo ln -sf "$PWD/lwp.sh" /usr/local/bin/lwp

echo ""
echo "Done. Run 'lwp set <video>' or 'lwp tui' to get started."
