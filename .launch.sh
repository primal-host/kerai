#!/bin/bash
# Launch script for kerai
# Repo name override: "kerai" instead of auto-derived "primal-kerai"
# Usage: ./.launch.sh [commit message]

set -euo pipefail

GITEA_HOST="https://gitea.primal.host"
GITEA_USER="primal"
MSG="${1:-}"

if [ -n "$MSG" ]; then
    git add -A
    git commit -m "$MSG"
fi

GITEA_TOKEN=$(cat ~/.claude/credentials/gitea-token.txt | tr -d '[:space:]')
git -c "http.extraHeader=Authorization: token $GITEA_TOKEN" push gitea main
