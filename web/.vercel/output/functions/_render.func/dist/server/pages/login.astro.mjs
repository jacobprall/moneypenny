/* empty css                                    */
import { e as createComponent, k as renderComponent, l as renderScript, r as renderTemplate, h as createAstro, m as maybeRenderHead, g as addAttribute } from '../chunks/astro/server_BWUWzUga.mjs';
import 'piccolore';
import { $ as $$Base } from '../chunks/Base_BNxIXlGo.mjs';
export { renderers } from '../renderers.mjs';

const $$Astro = createAstro();
const prerender = false;
const $$Login = createComponent(async ($$result, $$props, $$slots) => {
  const Astro2 = $$result.createAstro($$Astro, $$props, $$slots);
  Astro2.self = $$Login;
  const user = Astro2.locals.user;
  if (user) {
    return Astro2.redirect("/dashboard");
  }
  const redirect = Astro2.url.searchParams.get("redirect") ?? "/dashboard";
  return renderTemplate`${renderComponent($$result, "Base", $$Base, { "title": "Sign In \u2014 Moneypenny" }, { "default": async ($$result2) => renderTemplate` ${maybeRenderHead()}<div class="flex min-h-screen items-center justify-center px-6"> <div class="w-full max-w-sm"> <a href="/" class="mb-12 block text-center text-[15px] font-semibold tracking-tight">
moneypenny
</a> <div class="rounded-lg border border-zinc-800 bg-zinc-900/50 p-8"> <h1 class="text-xl font-bold tracking-tight">Sign in</h1> <p class="mt-2 text-[13px] text-zinc-400">
Sign in with GitHub to get started.
</p> <button id="github-login" class="mt-8 flex w-full items-center justify-center gap-2 rounded-md bg-zinc-50 px-4 py-2.5 text-sm font-medium text-zinc-950 transition-colors hover:bg-zinc-200"${addAttribute(redirect, "data-redirect")}> <svg class="h-4 w-4" viewBox="0 0 24 24" fill="currentColor"> <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"></path> </svg>
Continue with GitHub
</button> </div> </div> </div> ` })} ${renderScript($$result, "/Users/colossus/Desktop/untitled folder 2/moneypenny/web/src/pages/login.astro?astro&type=script&index=0&lang.ts")}`;
}, "/Users/colossus/Desktop/untitled folder 2/moneypenny/web/src/pages/login.astro", void 0);

const $$file = "/Users/colossus/Desktop/untitled folder 2/moneypenny/web/src/pages/login.astro";
const $$url = "/login";

const _page = /*#__PURE__*/Object.freeze(/*#__PURE__*/Object.defineProperty({
  __proto__: null,
  default: $$Login,
  file: $$file,
  prerender,
  url: $$url
}, Symbol.toStringTag, { value: 'Module' }));

const page = () => _page;

export { page };
