import { isValidElement, type ReactNode } from "react";
import Image from "next/image";
import Link from "next/link";
import { notFound } from "next/navigation";

import { ArticleTracking } from "@/components/article-tracking";
import { ContentCardThumbnail } from "@/components/my-components/content-card-thumbnail";
import {
  ArticleSidebar,
  type TocSection,
} from "@/components/my-components/toc-and-share";
import { Typeset } from "@/components/ui/typeset";
import { categoryLabel } from "@/lib/article-categories";
import { serializeJsonLd } from "@/lib/json-ld";
import { pageMetadata } from "@/lib/seo";
import { source } from "@/lib/source";
import { siteConfig } from "@/lib/site";
import { mdxComponents } from "@/mdx-components";

import { ArticleClosingCta } from "./article-closing-cta";

/**
 * Single article page — ported 1:1 from the Rustify `/articles/[slug]`
 * design:
 *
 *   HERO ROW   breadcrumb chips + H1 + author on the left, the generated
 *              thumbnail card on the right.
 *   BODY GRID  sticky sidebar (scroll-spy TOC + share row) beside the MDX
 *              rich-text column wrapped in `<Typeset>`.
 *   CLOSING    the shared `ClosingCta` block.
 *
 * TOC sections are the `##` headings, read straight off the fumadocs
 * `page.data.toc`. SEO: self-canonical + article OG (in `generateMetadata`)
 * plus a JSON-LD `BlogPosting` + `BreadcrumbList` array.
 */

/**
 * fumadocs `toc[].title` is a `ReactNode` (a heading can carry inline
 * markup), not a plain string — `String(node)` yields "[object Object]".
 * Walk it for its text content.
 */
function tocText(node: ReactNode): string {
  if (node == null || node === false) return "";
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(tocText).join("");
  if (isValidElement(node)) {
    return tocText((node.props as { children?: ReactNode }).children);
  }
  return "";
}

// Rustify `.breadcrumb_link` — pill chip.
const CHIP =
  "rounded-full border border-border bg-muted px-[9.72px] py-[5.83px] text-[11.67px] font-medium uppercase leading-[16.33px] tracking-[-0.007em] text-foreground transition-colors duration-200 hover:bg-muted/70";

export function generateStaticParams() {
  return source.getPages().map((page) => ({
    slug: page.slugs[page.slugs.length - 1],
  }));
}

export async function generateMetadata(props: PageProps<"/articles/[slug]">) {
  const params = await props.params;
  const page = source.getPage([params.slug]);

  if (!page) {
    notFound();
  }

  return {
    ...pageMetadata(page.data.title, page.data.description ?? page.data.title, page.url, {
      publishedTime: page.data.date,
      authors: [page.data.author],
      tags: page.data.tags,
    }),
    authors: [{ name: page.data.author, url: siteConfig.author.url }],
    keywords: page.data.tags,
  };
}

export default async function ArticlePage(props: PageProps<"/articles/[slug]">) {
  const params = await props.params;
  const page = source.getPage([params.slug]);

  if (!page) {
    notFound();
  }

  const { title, description, date, image, author, authorImage, category } = page.data;
  const MDX = page.data.body;
  const slug = params.slug;
  const path = page.url;
  const url = `${siteConfig.url}${path}`;

  // Sidebar TOC — the `##` headings only, matching the Rustify clone. `url` is
  // `#<github-slugger-id>` as emitted on the rendered `<h2>`.
  const sections: TocSection[] = page.data.toc
    .filter((item) => item.depth === 2)
    .map((item) => ({
      id: item.url.replace(/^#/, ""),
      label: tocText(item.title),
    }));

  const publishedDate = date ? `${date}T00:00:00+00:00` : undefined;

  const jsonLd = [
    {
      "@context": "https://schema.org",
      "@type": "BlogPosting",
      "@id": `${url}#article`,
      headline: title,
      description,
      url,
      image: [`${siteConfig.url}${image}`],
      datePublished: publishedDate,
      dateModified: publishedDate,
      author: { "@type": "Person", name: author },
      publisher: { "@id": `${siteConfig.url}/#organization` },
      mainEntityOfPage: { "@type": "WebPage", "@id": url },
    },
    {
      "@context": "https://schema.org",
      "@type": "BreadcrumbList",
      "@id": `${url}#breadcrumb`,
      itemListElement: [
        { "@type": "ListItem", position: 1, name: "Home", item: siteConfig.url },
        {
          "@type": "ListItem",
          position: 2,
          name: "Articles",
          item: `${siteConfig.url}/articles`,
        },
        {
          "@type": "ListItem",
          position: 3,
          name: categoryLabel(category),
          item: `${siteConfig.url}/articles/category/${category}`,
        },
        { "@type": "ListItem", position: 4, name: title, item: url },
      ],
    },
  ];

  return (
    <>
      <ArticleTracking />
      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: serializeJsonLd(jsonLd) }}
      />

      <div className="mx-auto max-w-[1200px] px-6 pb-24 pt-16">
        {/* HERO ROW — breadcrumb + title + author on the left, visual on the right */}
        <header className="grid gap-10 md:grid-cols-[minmax(0,1fr)_auto] md:items-start">
          <div className="flex flex-col gap-6">
            <nav className="flex items-center gap-[7.78px]" aria-label="Breadcrumb">
              <Link href="/articles" className={CHIP}>
                Articles
              </Link>
              <span className="text-[13.61px] font-medium leading-none text-muted-foreground">
                /
              </span>
              <Link href={`/articles/category/${category}`} className={CHIP}>
                {categoryLabel(category)}
              </Link>
            </nav>

            <h1 className="text-[38.89px] font-semibold leading-[42.78px] tracking-[-0.042em] text-foreground">
              {title}
            </h1>

            <div className="flex w-fit items-center gap-3">
              <Image
                src={authorImage}
                alt={author}
                width={44}
                height={44}
                className="size-11 shrink-0 rounded-full object-cover"
              />
              <span className="flex flex-col">
                <span className="text-[14px] font-medium text-foreground">{author}</span>
                {date && (
                  <span className="text-[13px] text-muted-foreground">
                    {new Date(date).toLocaleDateString("en-US", {
                      year: "numeric",
                      month: "long",
                      day: "numeric",
                    })}
                  </span>
                )}
              </span>
            </div>
          </div>

          <div
            data-header-invert
            className="relative aspect-[1200/675] w-full overflow-hidden rounded-[16px] bg-foreground md:w-[420px] md:shrink-0"
          >
            <ContentCardThumbnail title={title} image={image} size="feature" priority />
          </div>
        </header>

        {/* BODY GRID — sticky sidebar (TOC + share) beside the MDX rich-text column */}
        <div className="mt-16 grid gap-12 lg:grid-cols-[262px_minmax(0,1fr)]">
          <ArticleSidebar
            sections={sections}
            shareUrl={url}
            shareTitle={title}
            trackingSlug={slug}
          />

          <article className="min-w-0">
            <Typeset>
              <MDX components={mdxComponents} />
            </Typeset>
          </article>
        </div>

        <ArticleClosingCta articleSlug={slug} path={path} />
      </div>
    </>
  );
}
