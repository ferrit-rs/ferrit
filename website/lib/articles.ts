import { source } from "@/lib/source";
import type { ArticleCategory } from "@/lib/article-categories";

/**
 * Flat, serialisable view of an article for the `/articles` hub + category
 * pages. Derived from the fumadocs `source` — no MDX body, safe to hand from a
 * server component down to the `"use client"` listing.
 */
export type ArticleSummary = {
  slug: string;
  url: string;
  title: string;
  /** label for the generated poster thumbnail — just the title for now */
  thumbnailLabel: string;
  description: string;
  image: string;
  category: ArticleCategory;
  date?: string;
};

function sortByDateDesc(a: ArticleSummary, b: ArticleSummary) {
  const da = a.date ? new Date(a.date).getTime() : 0;
  const db = b.date ? new Date(b.date).getTime() : 0;
  return db - da;
}

export function getArticleSummaries(): ArticleSummary[] {
  return source
    .getPages()
    .map((page) => {
      const slug = page.slugs[page.slugs.length - 1];
      return {
        slug,
        url: page.url,
        title: page.data.title,
        thumbnailLabel: page.data.title,
        description: page.data.description ?? page.data.title,
        image: page.data.image,
        category: page.data.category,
        date: page.data.date,
      } satisfies ArticleSummary;
    })
    .sort(sortByDateDesc);
}

export function getArticleSummariesByCategory(
  category: ArticleCategory,
): ArticleSummary[] {
  return getArticleSummaries().filter((a) => a.category === category);
}
