/* empty css                                    */
import { e as createComponent, k as renderComponent, l as renderScript, r as renderTemplate, h as createAstro, m as maybeRenderHead, g as addAttribute } from '../chunks/astro/server_BWUWzUga.mjs';
import 'piccolore';
import { $ as $$Base } from '../chunks/Base_DoWXUrLv.mjs';
export { renderers } from '../renderers.mjs';

const $$Astro = createAstro();
const prerender = false;
const $$Pricing = createComponent(async ($$result, $$props, $$slots) => {
  const Astro2 = $$result.createAstro($$Astro, $$props, $$slots);
  Astro2.self = $$Pricing;
  const user = Astro2.locals.user;
  const checkout = Astro2.url.searchParams.get("checkout");
  const plans = [
    {
      id: "starter",
      name: "Starter",
      price: "$19",
      description: "One agent, persistent memory, policy governance.",
      features: ["1 agent", "1,000 facts", "500 searches/mo", "GitHub support"]
    },
    {
      id: "pro",
      name: "Pro",
      price: "$49",
      description: "Multiple agents with shared knowledge and sync.",
      features: [
        "5 agents",
        "10,000 facts",
        "5,000 searches/mo",
        "CRDT sync",
        "Priority support"
      ],
      popular: true
    },
    {
      id: "team",
      name: "Team",
      price: "$149",
      description: "Full fleet with advanced governance and audit.",
      features: [
        "20 agents",
        "50,000 facts",
        "25,000 searches/mo",
        "CRDT sync",
        "Advanced policy engine",
        "Dedicated support"
      ]
    }
  ];
  return renderTemplate`${renderComponent($$result, "Base", $$Base, { "title": "Pricing \u2014 Moneypenny" }, { "default": async ($$result2) => renderTemplate` ${maybeRenderHead()}<nav class="fixed top-0 z-50 w-full border-b border-zinc-800/50 bg-[#09090b]/80 backdrop-blur-xl"> <div class="mx-auto flex h-14 max-w-5xl items-center justify-between px-6"> <a href="/" class="text-[15px] font-semibold tracking-tight">moneypenny</a> <div class="flex items-center gap-5"> <a href="/docs/" class="text-[13px] text-zinc-400 transition-colors hover:text-zinc-50">Docs</a> <a href="https://github.com/jacobprall/moneypenny" class="text-[13px] text-zinc-400 transition-colors hover:text-zinc-50">GitHub</a> ${user ? renderTemplate`<a href="/dashboard" class="rounded-md bg-zinc-50 px-3.5 py-1.5 text-[13px] font-medium text-zinc-950 transition-colors hover:bg-zinc-200">Dashboard</a>` : renderTemplate`<a href="/login" class="rounded-md bg-zinc-50 px-3.5 py-1.5 text-[13px] font-medium text-zinc-950 transition-colors hover:bg-zinc-200">Sign In</a>`} </div> </div> </nav> <section class="pt-32 pb-24 sm:pt-44 sm:pb-32"> <div class="mx-auto max-w-5xl px-6"> <div class="text-center"> <h1 class="text-3xl font-bold tracking-tight sm:text-5xl">
Simple, flat-rate pricing
</h1> <p class="mt-4 text-[17px] text-zinc-400">
Bring your own LLM key. Pay for the platform, not tokens.
</p> </div> ${checkout === "cancelled" && renderTemplate`<div class="mx-auto mt-8 max-w-md rounded-md border border-yellow-900/50 bg-yellow-900/10 px-4 py-3 text-center text-[13px] text-yellow-200/80">
Checkout was cancelled. Pick a plan to try again.
</div>`} <div class="mt-16 grid gap-6 lg:grid-cols-3"> ${plans.map((plan) => renderTemplate`<div${addAttribute([
    "relative flex flex-col rounded-lg border p-8",
    plan.popular ? "border-zinc-600 bg-zinc-900/80" : "border-zinc-800 bg-zinc-900/30"
  ], "class:list")}> ${plan.popular && renderTemplate`<span class="absolute -top-3 left-1/2 -translate-x-1/2 rounded-full bg-zinc-50 px-3 py-0.5 text-[11px] font-semibold uppercase tracking-wider text-zinc-950">
Popular
</span>`} <h3 class="text-lg font-semibold">${plan.name}</h3> <p class="mt-1 text-[13px] text-zinc-400">${plan.description}</p> <div class="mt-6"> <span class="text-4xl font-bold tracking-tight">${plan.price}</span> <span class="text-[13px] text-zinc-500">/month</span> </div> <ul class="mt-8 flex-1 space-y-3"> ${plan.features.map((f) => renderTemplate`<li class="flex items-center gap-2 text-[13px] text-zinc-300"> <svg class="h-3.5 w-3.5 shrink-0 text-zinc-500" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5"> <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"></path> </svg> ${f} </li>`)} </ul> <button${addAttribute([
    "checkout-btn mt-8 w-full rounded-md px-4 py-2.5 text-sm font-medium transition-colors",
    plan.popular ? "bg-zinc-50 text-zinc-950 hover:bg-zinc-200" : "border border-zinc-800 text-zinc-300 hover:border-zinc-600 hover:text-zinc-50"
  ], "class:list")}${addAttribute(plan.id, "data-plan")}>
Get started
</button> </div>`)} </div> <p class="mt-12 text-center text-[13px] text-zinc-500">
All plans include BYOK (bring your own key) for LLM access.
        You pay your LLM provider directly.
</p> </div> </section> ` })} ${renderScript($$result, "/Users/colossus/Desktop/untitled folder 2/moneypenny/web/src/pages/pricing.astro?astro&type=script&index=0&lang.ts")}`;
}, "/Users/colossus/Desktop/untitled folder 2/moneypenny/web/src/pages/pricing.astro", void 0);

const $$file = "/Users/colossus/Desktop/untitled folder 2/moneypenny/web/src/pages/pricing.astro";
const $$url = "/pricing";

const _page = /*#__PURE__*/Object.freeze(/*#__PURE__*/Object.defineProperty({
  __proto__: null,
  default: $$Pricing,
  file: $$file,
  prerender,
  url: $$url
}, Symbol.toStringTag, { value: 'Module' }));

const page = () => _page;

export { page };
