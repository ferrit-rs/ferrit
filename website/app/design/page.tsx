import type { Metadata } from "next";
import { Check } from "lucide-react";

import { BracketHeading } from "@/components/my-components/bracket-heading";
import { ClosingCta } from "@/components/my-components/closing-cta";
import { pageMetadata } from "@/lib/seo";

const title = "Ferrit design choices";
const description =
  "The technical and product decisions behind Ferrit's terminal experience.";
export const metadata: Metadata = pageMetadata(title, description, "/design");

const DECISIONS = [
  [
    "Git remains the source of truth",
    "Ferrit adds a surface around Git instead of inventing a second repository model. The user should be able to understand how an action maps to familiar Git state.",
  ],
  [
    "Reads and commands have different needs",
    "libgit2 gives structured reads for the UI. The real Git binary handles configuration, hooks, diff drivers, and behavior where compatibility matters more than abstraction purity.",
  ],
  [
    "The TUI is not the domain",
    "Ratatui renders state and collects intent. Headless domain modules own repository behavior so the important rules can be tested without a terminal frame.",
  ],
  [
    "Every process edge is deliberate",
    "Git commands, GitHub CLI, askpass, configuration, output logging, and secret redaction pass through explicit boundaries. Hidden process behavior is a reliability risk.",
  ],
  [
    "Workflow tests beat isolated confidence",
    "Replay scripts and integration tests exercise stage, commit, rebase, conflict, remote, dashboard, and settings flows as a user would encounter them.",
  ],
  [
    "Delivery is part of the design",
    "Documentation, release tags, strict lints, install instructions, an MIT license, and a public repository make the project usable beyond its author.",
  ],
] as const;

export default function DesignPage() {
  return (
    <div className="flex flex-1 flex-col">
      <section className="mx-auto w-full max-w-5xl px-4 py-16 sm:py-24">
        <div className="max-w-3xl">
          <BracketHeading as="h1" kicker="Design choices">
            The reasoning lives in the system.
          </BracketHeading>
          <p className="mt-7 max-w-2xl text-lg leading-8 text-muted-foreground">
            Ferrit is a project about trust at the boundary between a developer and Git.
            These decisions explain why the application has the shape it has, what it
            keeps explicit, and which conveniences it refuses to hide.
          </p>
        </div>
      </section>

      <section className="mx-auto w-full max-w-5xl px-4 py-16">
        <div className="grid gap-10 lg:grid-cols-[0.72fr_1.28fr] lg:gap-20">
          <div>
            <BracketHeading kicker="Decision log" muted>
              The final design makes trade-offs visible.
            </BracketHeading>
            <p className="mt-6 max-w-sm text-[15px] leading-7 text-muted-foreground">
              A useful project page should show more than a polished terminal screenshot.
              The decisions below connect the interface to the constraints underneath it.
            </p>
          </div>
          <div className="overflow-hidden rounded-3xl border border-border bg-card ring-1 ring-foreground/5">
            {DECISIONS.map(([heading, body], index) => (
              <article
                key={heading}
                className="grid gap-4 border-b border-border p-6 last:border-b-0 md:grid-cols-[56px_210px_1fr] md:items-start md:gap-6"
              >
                <span className="font-mono text-sm text-muted-foreground">
                  0{index + 1}
                </span>
                <h2 className="text-lg font-semibold tracking-tight">{heading}</h2>
                <p className="text-sm leading-6 text-muted-foreground">{body}</p>
              </article>
            ))}
          </div>
        </div>
      </section>

      <section className="mx-auto w-full max-w-3xl px-4 py-20 sm:py-28">
        <article>
          <BracketHeading kicker="In practice" muted>
            Familiar behavior, clearer surface.
          </BracketHeading>
          <div className="mt-8 space-y-6 text-[16px] leading-8 text-muted-foreground">
            <p>
              Ferrit begins with the behavior developers already know: a repository has
              state, Git changes that state, and the user needs to understand what will
              happen before confirming an action. The terminal interface gives that state
              a stable visual home.
            </p>
            <p>
              The application therefore separates intent from execution. A keyboard action
              expresses what the user wants. The domain layer decides how that intent
              becomes a repository operation. The process seam handles the external
              command, captures useful output, and keeps credentials out of the log.
            </p>
            <p>
              This split matters when behavior gets difficult. Rebases, conflicts,
              remotes, hooks, authentication, empty folders, and user configuration are
              not edge decorations. They are where a Git tool earns or loses trust.
            </p>
          </div>
        </article>

        <article className="mt-24 sm:mt-32">
          <BracketHeading kicker="The trade-offs" muted>
            Convenience stops where ambiguity starts.
          </BracketHeading>
          <div className="mt-8 space-y-6 text-[16px] leading-8 text-muted-foreground">
            <p>
              Ferrit does not try to replace Git's mental model with a private one. A
              shortcut is useful when it compresses repetitive work while preserving the
              user's ability to inspect the resulting state. It is harmful when it makes a
              destructive operation feel magical.
            </p>
            <p>
              That is why the project favors explicit confirmations, visible command
              outcomes, predictable keyboard flows, and small surfaces over a dashboard
              full of competing controls. The interface should help a developer decide,
              not decide silently on their behalf.
            </p>
            <p>
              The same discipline applies to Rust architecture. A reusable abstraction
              earns its place when it protects a real boundary or removes repeated risk.
              It does not earn its place because a type system can express it.
            </p>
          </div>
        </article>

        <article className="mt-24 sm:mt-32">
          <BracketHeading kicker="Quality bar" muted>
            What makes the result credible.
          </BracketHeading>
          <div className="mt-8 space-y-6 text-[16px] leading-8 text-muted-foreground">
            <p>
              A finished Ferrit release needs more than code that compiles. The project
              must be understandable to a contributor, useful to a terminal user, and
              honest about the boundaries where external systems can fail.
            </p>
            <p>
              That quality bar appears in the repository: strict lint policy, integration
              coverage, deterministic replay, flow specifications, documentation, release
              history, and a public license. Each artifact reduces the amount of trust the
              reader has to supply personally.
            </p>
            <div className="grid gap-3 not-prose sm:grid-cols-2">
              {[
                "Readable domain modules",
                "Explicit process seam",
                "Workflow-level tests",
                "Public release trail",
              ].map((item) => (
                <div
                  key={item}
                  className="flex items-center gap-3 rounded-xl bg-muted/60 px-4 py-3 text-sm font-medium text-foreground"
                >
                  <Check className="size-4 text-primary" />
                  {item}
                </div>
              ))}
            </div>
          </div>
        </article>
      </section>

      <section className="mx-auto w-full max-w-5xl px-4 pb-24">
        <ClosingCta
          heading="Want to inspect the reasoning?"
          body="Read the project overview, browse the public repository, or send Richard the system constraint you are working through."
          ctaLabel="Contact me"
          ctaHref="/contact"
          ctaClassName="h-12 rounded-xl px-6 text-base font-semibold shadow-lg shadow-primary/20"
        />
      </section>
    </div>
  );
}
