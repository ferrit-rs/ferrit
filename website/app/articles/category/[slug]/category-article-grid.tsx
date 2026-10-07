"use client";

import type { ArticleSummary } from "@/lib/articles";
import { ArticlePosterCard } from "@/components/my-components/article-poster-card";

/**
 * Static 3-col grid for `/articles/category/[slug]`. No search / chips / load
 * more — the whole category fits on one page. Cards still fire
 * `article_card_clicked` with `category` = this category slug.
 */
export function CategoryArticleGrid({
  articles,
  category,
}: {
  articles: ArticleSummary[];
  category: string;
}) {
  if (articles.length === 0) {
    return (
      <p className="py-16 text-center text-[15px] text-muted-foreground">
        No articles in this category yet.
      </p>
    );
  }

  return (
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
      {articles.map((a, i) => (
        <ArticlePosterCard
          key={a.slug}
          article={a}
          eventCategory={category}
          position={i + 1}
        />
      ))}
    </div>
  );
}
