"use client";

import Link from "next/link";

import { Button } from "@/components/ui/button";
import { capture, EVENTS } from "@/lib/posthog";

export function ContactCtaButton({
  href,
  children,
  className,
}: {
  href: string;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <Button
      className={className}
      size="lg"
      nativeButton={false}
      onClick={() => capture(EVENTS.CONTACT_CTA_CLICKED)}
      // biome-ignore lint/a11y/useAnchorContent: content is provided by Button's children via the render prop
      render={href.startsWith("/") ? <Link href={href} /> : <a href={href} />}
    >
      {children}
    </Button>
  );
}
