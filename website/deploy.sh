#!/usr/bin/env bash
# Deploy this site to Vercel (prod).
set -euo pipefail
cd "$(dirname "$0")"
[ -f .env.local ] && source .env.local
: "${VERCEL_ORG_ID:?Set VERCEL_ORG_ID in .env.local (see .env.example)}"
: "${VERCEL_PROJECT_ID:?Set VERCEL_PROJECT_ID in .env.local (see .env.example)}"
VERCEL_ORG_ID="$VERCEL_ORG_ID" VERCEL_PROJECT_ID="$VERCEL_PROJECT_ID" vercel --prod --yes
