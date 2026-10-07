/**
 * Canonical article-category taxonomy.
 *
 * Imported BOTH by the app (server components, tracking) AND by
 * `source.config.ts` at build time (to type the MDX `category:` frontmatter),
 * so it must stay dependency-free: no React, no "server-only", no Node APIs.
 *
 * `ARTICLE_CATEGORIES` is a closed `as const` tuple, so:
 *   - `ArticleCategory` is a union of string literals (no free-form strings)
 *   - `z.enum(ARTICLE_CATEGORIES)` in source.config.ts rejects any other value
 *   - the `/articles` `?category=` filter + card pills map 1:1 to a chip
 */

export const ARTICLE_CATEGORIES = ["guides", "comparisons", "career"] as const;

export type ArticleCategory = (typeof ARTICLE_CATEGORIES)[number];

export type CategoryMeta = {
  /** Short tagline (breadcrumb-adjacent). */
  tagline: string;
  /** 2-3 line blurb — the "Browse Categories" rail card body + category page intro. */
  description: string;
};

export const CATEGORY_META: Record<ArticleCategory, CategoryMeta> = {
  guides: {
    tagline: "Start here",
    description:
      "Step-by-step guides for writing clear, reliable Rust and shipping useful backend systems.",
  },
  comparisons: {
    tagline: "Side by side",
    description: "Practical trade-offs between Rust tools, frameworks, and career paths.",
  },
  career: {
    tagline: "Grow",
    description:
      "Notes on learning Rust, building proof of work, and moving toward better engineering roles.",
  },
};

/**
 * `"career-switch"` -> `"Career Switch"`. Splits on `-`/`_`/whitespace and
 * title-cases each word. Generic — works for any kebab slug.
 */
function slugToTitle(slug: string): string {
  return slug
    .split(/[-_\s]+/)
    .filter(Boolean)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

/**
 * Display label for a category. Add an entry here only when title-casing is
 * wrong for a slug (acronyms, slashes), e.g. `"ai-ml": "AI/ML"`.
 */
const LABEL_OVERRIDES: Partial<Record<ArticleCategory, string>> = {};

export function categoryLabel(category: ArticleCategory): string {
  return LABEL_OVERRIDES[category] ?? slugToTitle(category);
}

export function isArticleCategory(value: string): value is ArticleCategory {
  return (ARTICLE_CATEGORIES as readonly string[]).includes(value);
}
