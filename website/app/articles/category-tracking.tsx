"use client";

import { useEffect } from "react";

import { capture, EVENTS } from "@/lib/posthog";

/**
 * Fires `article_category_viewed` once on mount. Used by the `/articles` hub
 * (`category="all"`, source "hub") and by `/articles/category/[slug]` (real
 * category key, source "category_page"). Renders nothing.
 */
export function CategoryTracking({
  category,
  articleCount,
  source = "hub",
}: {
  category: string;
  articleCount: number;
  source?: "hub" | "category_page";
}) {
  useEffect(() => {
    capture(EVENTS.ARTICLE_CATEGORY_VIEWED, {
      category,
      article_count: articleCount,
      source,
    });
  }, [category, articleCount, source]);

  return null;
}
