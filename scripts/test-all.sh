#!/usr/bin/env bash
# Full test suite. Set WH_DOCKER=1 to include SSH integration tests
# (requires: docker compose -f tests/docker/docker-compose.yml up -d --build).
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "${WH_DOCKER:-}" == "1" ]]; then
  cargo test --workspace -- --include-ignored
else
  cargo test --workspace
fi
npm --prefix ui test -- --run
npx --prefix ui tsc -p ui
(cd ui && npx playwright test)
