import { defineConfig, defineDocs } from "fumadocs-mdx/config";
import { pageSchema } from "fumadocs-core/source/schema";
import rehypePrettyCode from "rehype-pretty-code";
import { z } from "zod";

import { ARTICLE_CATEGORIES } from "./lib/article-categories";
import { siteConfig } from "./lib/site";

export default defineConfig({
  mdxOptions: {
    rehypePlugins: (plugins) => {
      plugins.shift();
      plugins.push([
        rehypePrettyCode,
        {
          theme: {
            dark: "github-dark",
            light: "github-light-default",
          },
        },
      ]);

      return plugins;
    },
  },
});

export const articles = defineDocs({
  dir: "content/articles",
  docs: {
    schema: pageSchema.extend({
      date: z.string().optional(),
      tags: z.array(z.string()).optional(),
      category: z.enum(ARTICLE_CATEGORIES).default("guides"),
      image: z.string().default("/images/default-article.webp"),
      author: z.string().default(siteConfig.author.name),
      authorImage: z.string().default(siteConfig.author.image),
    }),
  },
});
