"use client";

import { ClosingCta } from "@/components/my-components/closing-cta";
import { capture, EVENTS } from "@/lib/posthog";

/**
 * Client wrapper around the presentational `ClosingCta` for a single article.
 * Fires `article_cta_clicked { article_slug, path }` before navigation.
 */
export function ArticleClosingCta({
  articleSlug,
  path,
}: {
  articleSlug: string;
  path: string;
}) {
  return (
    <ClosingCta
      onCtaClick={() =>
        capture(EVENTS.ARTICLE_CTA_CLICKED, { article_slug: articleSlug, path })
      }
    />
  );
}
