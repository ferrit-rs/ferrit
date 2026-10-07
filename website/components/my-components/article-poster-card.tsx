"use client";

import Link from "next/link";

import type { ArticleSummary } from "@/lib/articles";
import { categoryLabel } from "@/lib/article-categories";
import { capture, EVENTS } from "@/lib/posthog";
import { ContentCardThumbnail } from "@/components/my-components/content-card-thumbnail";

/**
 * The 3-col grid card — shared by the `/articles` listing and the
 * `/articles/category/[slug]` grid. Fires `article_card_clicked` with the
 * active filter context + 1-indexed position before navigating.
 */
export function ArticlePosterCard({
  article,
  eventCategory,
  position,
}: {
  article: ArticleSummary;
  eventCategory: string;
  position: number;
}) {
  return (
    <Link
      href={article.url}
      onClick={() =>
        capture(EVENTS.ARTICLE_CARD_CLICKED, {
          category: eventCategory,
          article_slug: article.slug,
          position,
        })
      }
      className="group block rounded-2xl border border-border bg-card p-2 ring-1 ring-foreground/5 transition-transform duration-200 hover:-translate-y-0.5"
    >
      <div className="relative aspect-[1200/630] w-full overflow-hidden rounded-xl bg-muted">
        <ContentCardThumbnail
          title={article.thumbnailLabel}
          image={article.image}
          size="grid"
        />
      </div>
      <div className="flex flex-col gap-2 px-3 pb-3 pt-4">
        <span className="w-fit rounded-md border border-border bg-muted px-1.5 py-px text-[12px] font-medium uppercase tracking-wide text-muted-foreground">
          {categoryLabel(article.category)}
        </span>
        <h3 className="line-clamp-3 text-[20px] font-semibold leading-[1.15] tracking-tight text-foreground">
          {article.title}
        </h3>
        <p className="line-clamp-2 text-[15px] leading-[1.45] text-muted-foreground">
          {article.description}
        </p>
      </div>
    </Link>
  );
}
