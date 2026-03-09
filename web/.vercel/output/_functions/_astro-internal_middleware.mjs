import { d as defineMiddleware, s as sequence } from './chunks/index_DKieToIH.mjs';
import { c as createSupabaseServer } from './chunks/supabase_BGZUdxY1.mjs';
import 'es-module-lexer';
import './chunks/astro-designed-error-pages_B1PHeVg0.mjs';
import 'piccolore';
import './chunks/astro/server_BWUWzUga.mjs';
import 'clsx';

const PROTECTED = ["/dashboard", "/pricing"];
const onRequest$1 = defineMiddleware(async (context, next) => {
  const supabase = createSupabaseServer(
    context.cookies,
    context.request.headers.get("cookie") ?? ""
  );
  const {
    data: { user }
  } = await supabase.auth.getUser();
  context.locals.user = user;
  context.locals.supabase = supabase;
  const isProtected = PROTECTED.some(
    (p) => context.url.pathname.startsWith(p)
  );
  if (isProtected && !user) {
    return context.redirect(
      `/login?redirect=${encodeURIComponent(context.url.pathname)}`
    );
  }
  return next();
});

const onRequest = sequence(
	
	onRequest$1
	
);

export { onRequest };
