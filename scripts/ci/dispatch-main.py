#!/usr/bin/env python3
"""Send a main-chain dispatch; keep the secret off process argv and diagnostics."""
import os
import sys
import urllib.request
from pathlib import Path

workflow, payload = sys.argv[1:]
if workflow not in {'test.yml', 'docsite-deploy.yml'}:
    sys.exit('unsupported main-chain dispatch')
request = urllib.request.Request(
    f'https://git.integrolabs.net/api/v1/repos/{os.environ["GITHUB_REPOSITORY"]}/actions/workflows/{workflow}/dispatches',
    data=Path(payload).read_bytes(), method='POST',
    headers={'Authorization': 'token ' + os.environ['BUILD_REPO_TOKEN'],
             'Accept': 'application/json', 'Content-Type': 'application/json'})
try:
    with urllib.request.urlopen(request, timeout=30) as response:
        response.read()
except Exception:
    sys.exit('::error::Main-chain dispatch failed; see server Actions logs.')
