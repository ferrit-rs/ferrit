"use client";

import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";

/**
 * FAQ block: a single-open accordion over a `{ q, a }[]` list. Client component
 * (the accordion primitive owns open/close state). Content is fill-in prompts;
 * swap the `FAQ_ITEMS` default or pass your own `items`.
 */

export type FaqItem = { q: string; a: string };

const FAQ_ITEMS: FaqItem[] = [
  {
    q: "Write question one here?",
    a: "Answer slot: two or three sentences. Address the objection directly, then point at the next step.",
  },
  {
    q: "Write question two here?",
    a: "Answer slot: two or three sentences. Address the objection directly, then point at the next step.",
  },
  {
    q: "Write question three here?",
    a: "Answer slot: two or three sentences. Address the objection directly, then point at the next step.",
  },
  {
    q: "Write question four here?",
    a: "Answer slot: two or three sentences. Address the objection directly, then point at the next step.",
  },
  {
    q: "Write question five here?",
    a: "Answer slot: two or three sentences. Address the objection directly, then point at the next step.",
  },
];

export function FaqAccordion({ items = FAQ_ITEMS }: { items?: FaqItem[] }) {
  return (
    <Accordion type="single" collapsible defaultValue="faq-0" className="w-full">
      {items.map((item, i) => (
        <AccordionItem key={item.q} value={`faq-${i}`}>
          <AccordionTrigger>{item.q}</AccordionTrigger>
          <AccordionContent>{item.a}</AccordionContent>
        </AccordionItem>
      ))}
    </Accordion>
  );
}
