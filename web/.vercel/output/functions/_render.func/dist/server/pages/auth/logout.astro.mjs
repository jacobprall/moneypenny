/* empty css                                       */
import { e as createComponent, h as createAstro } from '../../chunks/astro/server_BWUWzUga.mjs';
import 'piccolore';
import 'clsx';
export { renderers } from '../../renderers.mjs';

const $$Astro = createAstro();
const prerender = false;
const $$Logout = createComponent(async ($$result, $$props, $$slots) => {
  const Astro2 = $$result.createAstro($$Astro, $$props, $$slots);
  Astro2.self = $$Logout;
  const supabase = Astro2.locals.supabase;
  await supabase.auth.signOut();
  return Astro2.redirect("/");
}, "/Users/colossus/Desktop/untitled folder 2/moneypenny/web/src/pages/auth/logout.astro", void 0);

const $$file = "/Users/colossus/Desktop/untitled folder 2/moneypenny/web/src/pages/auth/logout.astro";
const $$url = "/auth/logout";

const _page = /*#__PURE__*/Object.freeze(/*#__PURE__*/Object.defineProperty({
	__proto__: null,
	default: $$Logout,
	file: $$file,
	prerender,
	url: $$url
}, Symbol.toStringTag, { value: 'Module' }));

const page = () => _page;

export { page };
