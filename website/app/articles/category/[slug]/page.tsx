import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";

import { getArticleSummariesByCategory } from "@/lib/articles";
import { pageMetadata } from "@/lib/seo";
import {
  ARTICLE_CATEGORIES,
  CATEGORY_META,
  categoryLabel,
  isArticleCategory,
} from "@/lib/article-categories";
import { BracketHeading } from "@/components/my-components/bracket-heading";

import { CategoryTracking } from "../../category-tracking";
import { HubClosingCta } from "../../hub-closing-cta";
import { CategoryArticleGrid } from "./category-article-grid";

/**
 * `/articles/category/[slug]` — one dedicated, statically-generated page per
 * `ArticleCategory`. Intro block (BracketHeading + blurb) · full grid · CTA.
 * The `/articles` "Browse Categories" rail and the category pills link here.
 */

export function generateStaticParams() {
  return ARTICLE_CATEGORIES.map((slug) => ({ slug }));
}

export async function generateMetadata(
  props: PageProps<"/articles/category/[slug]">,
): Promise<Metadata> {
  const { slug } = await props.params;
  if (!isArticleCategory(slug)) return {};

  const label = categoryLabel(slug);
  const title = `${label} articles`;
  const description = CATEGORY_META[slug].description;
  return pageMetadata(title, description, `/articles/category/${slug}`);
}

export default async function CategoryPage(
  props: PageProps<"/articles/category/[slug]">,
) {
  const { slug } = await props.params;
  if (!isArticleCategory(slug)) {
    notFound();
  }

  const articles = getArticleSummariesByCategory(slug);
  const meta = CATEGORY_META[slug];

  return (
    <>
      <CategoryTracking
        category={slug}
        articleCount={articles.length}
        source="category_page"
      />

      <div className="mx-auto w-full max-w-[1200px] px-6 pb-24 pt-16">
        <nav className="mb-8 text-[13px] text-muted-foreground">
          <Link href="/articles" className="hover:underline">
            Articles
          </Link>
          <span className="px-1.5">/</span>
          <span className="text-foreground">{categoryLabel(slug)}</span>
        </nav>

        <header className="flex max-w-[640px] flex-col gap-4">
          <BracketHeading as="h1" kicker="Category">
            {categoryLabel(slug)}
          </BracketHeading>
          <p className="text-[18px] leading-[1.45] text-muted-foreground">
            {meta.description}
          </p>
          <p className="text-[13px] tabular-nums text-muted-foreground">
            {articles.length} article{articles.length === 1 ? "" : "s"}
          </p>
        </header>

        <div className="mt-14">
          <CategoryArticleGrid articles={articles} category={slug} />
        </div>

        <HubClosingCta hub="articles" context={slug} />
      </div>
    </>
  );
}
