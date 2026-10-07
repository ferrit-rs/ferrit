import path from "node:path";
import type { MetadataRoute } from "next";

import { source } from "@/lib/source";
import { siteConfig } from "@/lib/site";
import { getLastCommitDate } from "@/lib/git-date";
import { ARTICLE_CATEGORIES } from "@/lib/article-categories";

export default function sitemap(): MetadataRoute.Sitemap {
  const tags = new Set<string>();
  for (const page of source.getPages()) {
    for (const tag of page.data.tags ?? []) {
      tags.add(tag);
    }
  }
  const tagPages = [...tags].map((tag) => ({
    url: `${siteConfig.url}/tags/${encodeURIComponent(tag)}`,
    changeFrequency: "monthly" as const,
    priority: 0.4,
  }));

  const categoryPages = ARTICLE_CATEGORIES.map((slug) => ({
    url: `${siteConfig.url}/articles/category/${slug}`,
    changeFrequency: "weekly" as const,
    priority: 0.5,
  }));

  const articles = source.getPages().map((page) => {
    const slug = page.slugs[page.slugs.length - 1];
    const filePath = path.join("content/articles", `${slug}.mdx`);
    const lastModified =
      getLastCommitDate(filePath) ??
      (page.data.date ? new Date(page.data.date) : undefined);

    return {
      url: `${siteConfig.url}${page.url}`,
      lastModified,
      changeFrequency: "monthly" as const,
      priority: 0.8,
    };
  });

  return [
    {
      url: siteConfig.url,
      changeFrequency: "daily" as const,
      priority: 1,
    },
    {
      url: `${siteConfig.url}/articles`,
      changeFrequency: "weekly" as const,
      priority: 0.7,
    },
    {
      url: `${siteConfig.url}/contact`,
      changeFrequency: "yearly" as const,
      priority: 0.3,
    },
    {
      url: `${siteConfig.url}/about`,
      changeFrequency: "yearly" as const,
      priority: 0.5,
    },
    {
      url: `${siteConfig.url}/project`,
      changeFrequency: "yearly" as const,
      priority: 0.6,
    },
    {
      url: `${siteConfig.url}/design`,
      changeFrequency: "yearly" as const,
      priority: 0.5,
    },
    {
      url: `${siteConfig.url}/work`,
      changeFrequency: "monthly" as const,
      priority: 0.7,
    },
    ...categoryPages,
    ...articles,
    ...tagPages,
  ];
}
