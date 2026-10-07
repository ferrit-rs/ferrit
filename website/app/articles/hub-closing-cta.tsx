"use client";

import { ClosingCta } from "@/components/my-components/closing-cta";
import { capture, EVENTS } from "@/lib/posthog";

/**
 * Client wrapper around the presentational `ClosingCta` for the `/articles`
 * hub + `/articles/category/[slug]` pages. Fires `hub_cta_clicked { hub,
 * context }` before navigation.
 *
 *   hub     — which hub renders it: "articles"
 *   context — active filter at render: "all" | "<category>"
 */
export function HubClosingCta({ hub, context }: { hub: string; context: string }) {
  return (
    <ClosingCta onCtaClick={() => capture(EVENTS.HUB_CTA_CLICKED, { hub, context })} />
  );
}
