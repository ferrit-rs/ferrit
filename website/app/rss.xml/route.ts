import { source } from "@/lib/source";
import { siteConfig } from "@/lib/site";

function escapeXml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}

export function GET() {
  const pages = [...source.getPages()].sort((a, b) => {
    const dateA = a.data.date ? new Date(a.data.date).getTime() : 0;
    const dateB = b.data.date ? new Date(b.data.date).getTime() : 0;
    return dateB - dateA;
  });

  const items = pages
    .map((page) => {
      const url = `${siteConfig.url}${page.url}`;
      const pubDate = page.data.date ? new Date(page.data.date).toUTCString() : undefined;

      return `
    <item>
      <title>${escapeXml(page.data.title)}</title>
      <link>${url}</link>
      <guid>${url}</guid>
      ${page.data.description ? `<description>${escapeXml(page.data.description)}</description>` : ""}
      ${pubDate ? `<pubDate>${pubDate}</pubDate>` : ""}
      <author>${escapeXml(page.data.author)}</author>
    </item>`;
    })
    .join("");

  const feed = `<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>${escapeXml(siteConfig.name)}</title>
    <link>${siteConfig.url}</link>
    <description>${escapeXml(siteConfig.description)}</description>${items}
  </channel>
</rss>`;

  return new Response(feed, {
    headers: { "Content-Type": "application/xml; charset=utf-8" },
  });
}
