#!/usr/bin/env bash
set -euo pipefail

# Clone or update the Fortémi repository
# Params: INSTALL_DIR, BRANCH

INSTALL_DIR="${INSTALL_DIR:?INSTALL_DIR is required}"
BRANCH="${BRANCH:-main}"
REPO_URL="https://github.com/fortemi/fortemi.git"

if [ -d "${INSTALL_DIR}/.git" ]; then
    echo "Repository already exists at ${INSTALL_DIR}, updating..."
    cd "${INSTALL_DIR}"
    git fetch origin
    git checkout "${BRANCH}"
    git pull origin "${BRANCH}"
else
    echo "Cloning Fortémi (branch: ${BRANCH}) to ${INSTALL_DIR}..."
    mkdir -p "$(dirname "${INSTALL_DIR}")"
    git clone --branch "${BRANCH}" "${REPO_URL}" "${INSTALL_DIR}"
fi

echo "Repository ready at ${INSTALL_DIR}"
