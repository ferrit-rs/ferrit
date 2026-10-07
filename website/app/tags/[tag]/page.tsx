import { notFound } from "next/navigation";

import { source } from "@/lib/source";
import { pageMetadata } from "@/lib/seo";
import { ArticleCard } from "@/components/article-card";

function getTags() {
  const tags = new Set<string>();
  for (const page of source.getPages()) {
    for (const tag of page.data.tags ?? []) {
      tags.add(tag);
    }
  }
  return [...tags];
}

export function generateStaticParams() {
  return getTags().map((tag) => ({ tag }));
}

export async function generateMetadata(props: PageProps<"/tags/[tag]">) {
  const params = await props.params;
  const tag = decodeURIComponent(params.tag);
  const title = `#${tag}`;
  const description = `Articles tagged "${tag}".`;
  return pageMetadata(title, description, `/tags/${params.tag}`);
}

export default async function TagPage(props: PageProps<"/tags/[tag]">) {
  const params = await props.params;
  const tag = decodeURIComponent(params.tag);
  const pages = [...source.getPages()]
    .filter((page) => page.data.tags?.includes(tag))
    .sort((a, b) => {
      const dateA = a.data.date ? new Date(a.data.date).getTime() : 0;
      const dateB = b.data.date ? new Date(b.data.date).getTime() : 0;
      return dateB - dateA;
    });

  if (pages.length === 0) {
    notFound();
  }

  return (
    <div className="mx-auto flex w-full max-w-4xl flex-1 flex-col gap-8 px-4 py-16">
      <div className="flex flex-col gap-2">
        <h1 className="text-3xl font-semibold tracking-tight">#{tag}</h1>
        <p className="text-muted-foreground">
          {pages.length} article{pages.length > 1 ? "s" : ""} tagged &quot;{tag}&quot;.
        </p>
      </div>
      <div className="grid gap-4 sm:grid-cols-2">
        {pages.map((page) => (
          <ArticleCard
            key={page.url}
            url={page.url}
            title={page.data.title}
            description={page.data.description}
            date={page.data.date}
            tags={page.data.tags}
            image={page.data.image}
          />
        ))}
      </div>
    </div>
  );
}
