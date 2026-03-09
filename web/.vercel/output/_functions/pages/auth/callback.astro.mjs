/* empty css                                       */
import { e as createComponent, h as createAstro } from '../../chunks/astro/server_BWUWzUga.mjs';
import 'piccolore';
import 'clsx';
import { c as createSupabaseServer } from '../../chunks/supabase_BGZUdxY1.mjs';
export { renderers } from '../../renderers.mjs';

const $$Astro = createAstro();
const prerender = false;
const $$Callback = createComponent(async ($$result, $$props, $$slots) => {
  const Astro2 = $$result.createAstro($$Astro, $$props, $$slots);
  Astro2.self = $$Callback;
  const code = Astro2.url.searchParams.get("code");
  const redirect = Astro2.url.searchParams.get("redirect") ?? "/dashboard";
  if (code) {
    const supabase = createSupabaseServer(
      Astro2.cookies,
      Astro2.request.headers.get("cookie") ?? ""
    );
    await supabase.auth.exchangeCodeForSession(code);
  }
  return Astro2.redirect(redirect);
}, "/Users/colossus/Desktop/untitled folder 2/moneypenny/web/src/pages/auth/callback.astro", void 0);

const $$file = "/Users/colossus/Desktop/untitled folder 2/moneypenny/web/src/pages/auth/callback.astro";
const $$url = "/auth/callback";

const _page = /*#__PURE__*/Object.freeze(/*#__PURE__*/Object.defineProperty({
  __proto__: null,
  default: $$Callback,
  file: $$file,
  prerender,
  url: $$url
}, Symbol.toStringTag, { value: 'Module' }));

const page = () => _page;

export { page };
