# TODO on your side

Analytics and deploy are already wired into the code. You just need accounts + keys.

## 1. PostHog (analytics)

1. Create an account at posthog.com, create a project.
2. Get the **Project API Key**: `Settings → Project → Project API Key`.
3. Add to `.env.local` (copy `.env.example` first):
   ```
   NEXT_PUBLIC_POSTHOG_KEY=phc_xxx
   NEXT_PUBLIC_POSTHOG_HOST=https://us.i.posthog.com
   ```
4. Optional — if you want to query analytics data later via the API (not required for the site to track):
   - `Settings → search "personal api" → Create`, scope: read on "Query"
   - `POSTHOG_PERSONAL_API_KEY=phx_xxx`

Once the key is set, tracking is automatic: pageviews, code-block copies, article scroll depth, time on page, internal/outbound link clicks, contact CTA clicks, 404s. Bot traffic is filtered out automatically.

## 2. Vercel (deploy)

1. Create a Vercel account, install the CLI: `npm i -g vercel`.
2. From the project root, run `vercel link` and follow the prompts. This creates `.vercel/project.json` with your org/project IDs.
3. Add those IDs to `.env.local`:
   ```
   VERCEL_ORG_ID=team_xxx
   VERCEL_PROJECT_ID=prj_xxx
   ```
   (find them via `vercel project ls` or in `.vercel/project.json`)
4. Deploy: `./deploy.sh` — deploys straight to production.

## Notes

- `.env.local` is gitignored — create it yourself from `.env.example`, never commit it.
- None of this is required to build/run the site locally (`pnpm dev`, `pnpm build`) — only for live analytics and deploying to your own domain.
