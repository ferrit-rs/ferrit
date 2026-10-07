import type { Metadata } from "next";

import { pageMetadata } from "@/lib/seo";
import { getArticleSummaries } from "@/lib/articles";
import {
  ARTICLE_CATEGORIES,
  CATEGORY_META,
  categoryLabel,
  isArticleCategory,
  type ArticleCategory,
} from "@/lib/article-categories";
import { BracketHeading } from "@/components/my-components/bracket-heading";

import { FeaturedCard, PanelLabel, SpotlightRow } from "./article-highlights";
import { ArticlesListing, type CategoryChip } from "./articles-listing";
import { CategoryCarousel, type CategoryCard } from "./category-carousel";
import { CategoryTracking } from "./category-tracking";
import { HubClosingCta } from "./hub-closing-cta";

/**
 * `/articles` hub. Centered hero · FEATURED + SPOTLIGHT row · filterable
 * listing · "Browse Categories" rail · CTA. Taxonomy is the closed,
 * build-validated `ArticleCategory` frontmatter enum.
 *
 * `?category=<key>` deep-links a filter: it seeds `<ArticlesListing>`'s
 * selected chip set (multi-select). Invalid values are ignored. The dedicated
 * single-category page is `/articles/category/[slug]` — where the "Browse
 * Categories" rail points.
 */

const title = "Articles";
const description =
  "Practical articles about Rust engineering, technical trade-offs, and building a career with production skills.";
export const metadata: Metadata = pageMetadata(title, description, "/articles");

export default async function ArticlesHubPage({
  searchParams,
}: {
  searchParams: Promise<{ category?: string }>;
}) {
  const summaries = getArticleSummaries();
  const { category } = await searchParams;
  const initialCategory: ArticleCategory | null =
    category && isArticleCategory(category) ? category : null;

  const counts = new Map<ArticleCategory, number>();
  for (const a of summaries) {
    counts.set(a.category, (counts.get(a.category) ?? 0) + 1);
  }

  const chips: CategoryChip[] = ARTICLE_CATEGORIES.filter(
    (key) => (counts.get(key) ?? 0) > 0,
  ).map((key) => ({
    key,
    label: categoryLabel(key),
    count: counts.get(key) ?? 0,
  }));

  const categoryCards: CategoryCard[] = ARTICLE_CATEGORIES.filter(
    (key) => (counts.get(key) ?? 0) > 0,
  ).map((key) => ({
    key,
    label: categoryLabel(key),
    blurb: CATEGORY_META[key].description,
    count: counts.get(key) ?? 0,
  }));

  const featured = summaries[0];
  const spotlight = summaries.slice(1, 5);

  return (
    <>
      <CategoryTracking category="all" articleCount={summaries.length} />

      <div className="mx-auto w-full max-w-[1200px] px-6 pb-24 pt-16">
        <header className="mx-auto flex max-w-[680px] flex-col items-center gap-4 text-center">
          <BracketHeading as="h1" align="center" kicker="Articles">
            All Articles
          </BracketHeading>
          <p className="mx-auto max-w-[46ch] text-balance text-[18px] leading-[1.4] text-muted-foreground">
            {summaries.length} articles across {chips.length} categories: guides,
            comparisons and career.
          </p>
        </header>

        {featured ? (
          <section className="mt-16 grid gap-8 lg:grid-cols-[1.4fr_1fr]">
            <div className="rounded-3xl bg-muted/40 p-6">
              <PanelLabel>Featured</PanelLabel>
              <FeaturedCard article={featured} />
            </div>
            <div className="rounded-3xl bg-muted/40 p-6">
              <PanelLabel>Spotlight</PanelLabel>
              <div className="flex flex-col gap-3">
                {spotlight.map((a) => (
                  <SpotlightRow key={a.slug} article={a} />
                ))}
              </div>
            </div>
          </section>
        ) : null}

        <ArticlesListing
          articles={summaries}
          chips={chips}
          initialCategory={initialCategory}
        />

        <CategoryCarousel categories={categoryCards} />

        <HubClosingCta hub="articles" context="all" />
      </div>
    </>
  );
}
