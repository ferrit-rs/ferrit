"use client";

import { useRef, useState } from "react";
import { Check, Copy } from "lucide-react";

import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { capture, EVENTS } from "@/lib/posthog";

export function CopyCodeButton({ className, ...props }: React.ComponentProps<"pre">) {
  const preRef = useRef<HTMLPreElement>(null);
  const [copied, setCopied] = useState(false);

  async function handleCopy() {
    const code = preRef.current?.textContent ?? "";
    await navigator.clipboard.writeText(code);
    capture(EVENTS.CODE_BLOCK_COPIED);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  return (
    <div className="group relative">
      <pre
        ref={preRef}
        className={cn(
          "mt-4 mb-4 overflow-x-auto rounded-lg border bg-muted/50 p-4 text-sm",
          className,
        )}
        {...props}
      />
      <Button
        variant="ghost"
        size="icon"
        className="absolute top-2 right-2 size-7 opacity-0 transition-opacity group-hover:opacity-100"
        onClick={handleCopy}
      >
        {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
        <span className="sr-only">Copy code</span>
      </Button>
    </div>
  );
}
