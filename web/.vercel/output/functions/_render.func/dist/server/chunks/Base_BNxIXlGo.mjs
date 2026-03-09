import { e as createComponent, g as addAttribute, n as renderHead, o as renderSlot, r as renderTemplate, h as createAstro } from './astro/server_BWUWzUga.mjs';
import 'piccolore';
import 'clsx';
/* empty css                             */

const $$Astro = createAstro();
const $$Base = createComponent(($$result, $$props, $$slots) => {
  const Astro2 = $$result.createAstro($$Astro, $$props, $$slots);
  Astro2.self = $$Base;
  const {
    title,
    description = "Persistent memory, policy governance, and audit for AI agents \u2014 in a single SQLite file."
  } = Astro2.props;
  return renderTemplate`<html lang="en" class="scroll-smooth bg-[#09090b] text-zinc-50 antialiased"> <head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="description"${addAttribute(description, "content")}><meta name="generator"${addAttribute(Astro2.generator, "content")}><meta property="og:title"${addAttribute(title, "content")}><meta property="og:description"${addAttribute(description, "content")}><link rel="icon" type="image/svg+xml" href="/favicon.svg"><link rel="preconnect" href="https://fonts.googleapis.com"><link rel="preconnect" href="https://fonts.gstatic.com" crossorigin><link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700;800&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet"><title>${title}</title>${renderHead()}</head> <body class="min-h-screen font-sans"> ${renderSlot($$result, $$slots["default"])} </body></html>`;
}, "/Users/colossus/Desktop/untitled folder 2/moneypenny/web/src/layouts/Base.astro", void 0);

export { $$Base as $ };
