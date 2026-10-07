import type { Metadata } from "next";

import { siteConfig } from "@/lib/site";

type ArticleMetadata = {
  publishedTime?: string;
  modifiedTime?: string;
  authors?: string[];
  tags?: string[];
};

/** Shared metadata shape. Child routes must repeat OG/Twitter fields because Next does not deep-merge them. */
export function pageMetadata(
  title: string,
  description: string,
  path: string,
  article?: ArticleMetadata,
): Metadata {
  const url = `${siteConfig.url}${path}`;
  const openGraph = {
    title,
    description,
    url,
    siteName: siteConfig.name,
    images: [{ url: siteConfig.ogImage, width: 1280, height: 640, alt: title }],
    locale: "en_US",
    ...(article
      ? {
          type: "article" as const,
          publishedTime: article.publishedTime,
          modifiedTime: article.modifiedTime ?? article.publishedTime,
          authors: article.authors,
          tags: article.tags,
        }
      : { type: "website" as const }),
  };

  return {
    title,
    description,
    alternates: { canonical: url },
    openGraph,
    twitter: {
      card: "summary_large_image",
      title,
      description,
      images: [siteConfig.ogImage],
    },
  };
}
