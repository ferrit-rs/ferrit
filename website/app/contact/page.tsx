import type { Metadata } from "next";
import { ArrowRight } from "lucide-react";

import { ContactCtaButton } from "@/components/contact-cta-button";
import { BracketHeading } from "@/components/my-components/bracket-heading";
import { Button } from "@/components/ui/button";
import { pageMetadata } from "@/lib/seo";
import { siteConfig } from "@/lib/site";

const title = "Contact Richard";
const description =
  "Start a conversation about Rust systems, developer tooling, or data engineering.";
export const metadata: Metadata = pageMetadata(title, description, "/contact");

export default function ContactPage() {
  const email = siteConfig.contact.email;

  return (
    <div className="flex flex-1 flex-col">
      <section className="mx-auto grid w-full max-w-5xl gap-12 px-4 py-16 sm:py-24 lg:grid-cols-[0.85fr_1.15fr] lg:items-start lg:gap-20">
        <div className="flex flex-col gap-7">
          <BracketHeading as="h1" kicker="Contact">
            Have a Rust system worth discussing?
          </BracketHeading>
          <p className="max-w-lg text-lg leading-8 text-muted-foreground">
            Tell Richard what you are building, where the design feels difficult, and what
            the finished result needs to make easier. A short, concrete message is enough
            to begin.
          </p>
          <div className="flex flex-wrap gap-3">
            <ContactCtaButton
              href={`mailto:${email}`}
              className="h-12 rounded-xl px-6 text-base font-semibold shadow-lg shadow-primary/20"
            >
              Email Richard
              <ArrowRight />
            </ContactCtaButton>
            <Button
              variant="outline"
              size="lg"
              className="h-12 rounded-xl px-6 text-base font-semibold"
              nativeButton={false}
              // biome-ignore lint/a11y/useAnchorContent: Button children are forwarded through render.
              render={<a href={`mailto:${email}`} aria-label={`Email ${email}`} />}
            >
              {email}
            </Button>
          </div>
          <div className="rounded-2xl border border-border bg-card p-5 ring-1 ring-foreground/5">
            <p className="font-mono text-[12px] uppercase tracking-wide text-muted-foreground">
              What to include
            </p>
            <p className="mt-3 text-sm leading-6 text-muted-foreground">
              Project context, current constraint, expected users, relevant repository or
              documentation, and the decision you want to make next.
            </p>
          </div>
        </div>

        <div className="rounded-3xl border border-border bg-card p-5 shadow-sm ring-1 ring-foreground/5 sm:p-7">
          <div className="flex items-start justify-between gap-4 border-b border-border pb-5">
            <div>
              <p className="font-mono text-[12px] uppercase tracking-wide text-muted-foreground">
                Email draft
              </p>
              <h2 className="mt-2 text-2xl font-semibold tracking-tight">
                Start with the system.
              </h2>
            </div>
            <span className="rounded-full bg-muted px-3 py-1 font-mono text-[11px] uppercase tracking-wide text-muted-foreground">
              mailto
            </span>
          </div>
          <div className="mt-6 flex flex-col gap-5">
            <div className="grid gap-5 sm:grid-cols-2">
              <label className="flex flex-col gap-2 text-sm font-medium">
                Name
                <input
                  className="h-11 rounded-xl border border-border bg-background px-3 text-sm font-normal outline-none placeholder:text-muted-foreground focus:ring-2 focus:ring-ring"
                  placeholder="Your name"
                />
              </label>
              <label className="flex flex-col gap-2 text-sm font-medium">
                Email
                <input
                  type="email"
                  className="h-11 rounded-xl border border-border bg-background px-3 text-sm font-normal outline-none placeholder:text-muted-foreground focus:ring-2 focus:ring-ring"
                  placeholder="you@example.com"
                />
              </label>
            </div>
            <label className="flex flex-col gap-2 text-sm font-medium">
              Tell Richard about the project
              <textarea
                className="min-h-36 resize-y rounded-xl border border-border bg-background px-3 py-3 text-sm font-normal outline-none placeholder:text-muted-foreground focus:ring-2 focus:ring-ring"
                placeholder="What are you building, and where is the hard part?"
              />
            </label>
            <Button
              size="lg"
              className="h-12 rounded-xl px-6 text-base font-semibold"
              nativeButton={false}
              // biome-ignore lint/a11y/useAnchorContent: Button children are forwarded through render.
              render={<a href={`mailto:${email}`} aria-label="Send an email" />}
            >
              Open email
              <ArrowRight />
            </Button>
          </div>
        </div>
      </section>

      <section className="mx-auto w-full max-w-5xl px-4 pb-24">
        <BracketHeading kicker="Process" muted>
          A simple way to begin.
        </BracketHeading>
        <div className="mt-8 grid gap-4 md:grid-cols-3">
          {[
            [
              "Share the context.",
              "Describe the product, users, repository, and the result that matters.",
            ],
            [
              "Find the real constraint.",
              "Separate the technical symptom from the boundary or decision causing it.",
            ],
            [
              "Choose the next move.",
              "Leave with a practical next step, whether that means code, a review, or a clearer plan.",
            ],
          ].map(([step, body], index) => (
            <div
              key={step}
              className="rounded-2xl border border-border bg-card p-6 ring-1 ring-foreground/5"
            >
              <span className="font-mono text-sm text-muted-foreground">
                0{index + 1}
              </span>
              <h2 className="mt-8 text-lg font-semibold tracking-tight">{step}</h2>
              <p className="mt-3 text-sm leading-6 text-muted-foreground">{body}</p>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
