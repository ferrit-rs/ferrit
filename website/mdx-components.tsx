import type { MDXComponents } from "mdx/types";
import Link from "next/link";

import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { CopyCodeButton } from "@/components/copy-code-button";

function Heading({
  as: Comp,
  className,
  ...props
}: React.ComponentProps<"h1"> & { as: "h1" | "h2" | "h3" | "h4" }) {
  return (
    <Comp
      className={cn("scroll-m-24 font-semibold tracking-tight", className)}
      {...props}
    />
  );
}

export const mdxComponents: MDXComponents = {
  h1: (props) => <Heading as="h1" className="mt-10 text-3xl" {...props} />,
  h2: (props) => (
    <Heading as="h2" className="mt-10 border-b pb-2 text-2xl first:mt-0" {...props} />
  ),
  h3: (props) => <Heading as="h3" className="mt-8 text-xl" {...props} />,
  h4: (props) => <Heading as="h4" className="mt-6 text-lg" {...props} />,
  p: (props) => <p className="leading-7 [&:not(:first-child)]:mt-4" {...props} />,
  a: ({ href, ...props }) => (
    <Link
      href={href ?? "#"}
      className="font-medium text-primary underline underline-offset-4"
      {...props}
    />
  ),
  ul: (props) => <ul className="my-4 ml-6 list-disc [&>li]:mt-2" {...props} />,
  ol: (props) => <ol className="my-4 ml-6 list-decimal [&>li]:mt-2" {...props} />,
  blockquote: (props) => (
    <blockquote
      className="mt-4 border-l-2 border-border pl-6 italic text-muted-foreground"
      {...props}
    />
  ),
  pre: CopyCodeButton,
  code: (props) => (
    <code
      className="rounded bg-muted px-1.5 py-0.5 font-mono text-sm [pre_&]:bg-transparent [pre_&]:p-0"
      {...props}
    />
  ),
  hr: (props) => <hr className="my-8 border-border" {...props} />,
  Badge,
  Button,
  Card,
  CardContent,
};

export function useMDXComponents(components: MDXComponents): MDXComponents {
  return {
    ...mdxComponents,
    ...components,
  };
}
