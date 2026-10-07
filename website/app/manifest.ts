import type { MetadataRoute } from "next";

import { siteConfig } from "@/lib/site";

export default function manifest(): MetadataRoute.Manifest {
  return {
    name: siteConfig.name,
    short_name: siteConfig.name,
    description: siteConfig.description,
    start_url: "/",
    display: "standalone",
    background_color: "#ffffff",
    theme_color: "#18181b",
    icons: [
      { src: "/icon-192.webp", sizes: "192x192", type: "image/webp" },
      { src: "/icon-512.webp", sizes: "512x512", type: "image/webp" },
    ],
  };
}
