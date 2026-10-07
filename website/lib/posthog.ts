import posthog from "posthog-js";

export const EVENTS = {
  CONTACT_CTA_CLICKED: "contact_cta_clicked",
  CODE_BLOCK_COPIED: "code_block_copied",
  PAGE_NOT_FOUND: "page_not_found",
  ARTICLE_SCROLL_DEPTH: "article_scroll_depth",
  ARTICLE_TIME_ON_PAGE: "article_time_on_page",
  ARTICLE_OUTBOUND_LINK_CLICKED: "article_outbound_link_clicked",
  ARTICLE_INTERNAL_LINK_CLICKED: "article_internal_link_clicked",
  ARTICLE_TOC_CLICKED: "article_toc_clicked",
  ARTICLE_SHARE_CLICKED: "article_share_clicked",
  ARTICLE_CTA_CLICKED: "article_cta_clicked",
  ARTICLE_CATEGORY_VIEWED: "article_category_viewed",
  ARTICLE_CARD_CLICKED: "article_card_clicked",
  ARTICLE_CATEGORY_LOAD_MORE: "article_category_load_more",
  HUB_CTA_CLICKED: "hub_cta_clicked",
  HEADER_LINK_CLICKED: "header_link_clicked",
  FOOTER_LINK_CLICKED: "footer_link_clicked",
} as const;

export type PostHogEvent = (typeof EVENTS)[keyof typeof EVENTS];

export function slugFromPath(path: string): string {
  return path.replace(/^\/+/, "").replace(/^articles\//, "");
}

// Bots/crawlers to exclude. Never blocks page access — SEO crawlers still get full HTML.
// This only stops the JS SDK from sending events, so metrics reflect real visitors.
const BOT_UA_PATTERN =
  /bot|crawler|spider|crawling|Googlebot|Bingbot|Slurp|DuckDuckBot|Baiduspider|YandexBot|Applebot|facebookexternalhit|Twitterbot|LinkedInBot|WhatsApp|Discordbot|TelegramBot|Viber|Slack|GPTBot|Claude-Web|anthropic-ai|PerplexityBot|CCBot|Bytespider|Amazonbot|cohere-ai|AhrefsBot|SemrushBot|MJ12bot|DotBot|SiteAuditBot|rogerbot|ScreamingFrog|Lighthouse|HeadlessChrome|PhantomJS|Puppeteer|Playwright|Cypress|JSDOM|Selenium|wget|curl|python-requests|python-urllib|Go-http-client|libwww-perl|UptimeRobot|Pingdom|StatusCake|Site24x7|BetterUptime/i;

export function isLikelyBot(): boolean {
  if (typeof navigator === "undefined") return false;
  return BOT_UA_PATTERN.test(navigator.userAgent) || navigator.webdriver === true;
}

export function capture(event: PostHogEvent, props?: Record<string, unknown>) {
  if (typeof window === "undefined") return;
  posthog.capture(event, props);
}
