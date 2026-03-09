import { defineMiddleware } from "astro:middleware";
import { createSupabaseServer } from "./lib/supabase";

const PROTECTED = ["/dashboard", "/pricing"];

export const onRequest = defineMiddleware(async (context, next) => {
  const url = import.meta.env.PUBLIC_SUPABASE_URL;
  const key = import.meta.env.PUBLIC_SUPABASE_ANON_KEY;

  if (!url || !key) {
    return next();
  }

  const supabase = createSupabaseServer(
    context.cookies,
    context.request.headers.get("cookie") ?? ""
  );

  const {
    data: { user },
  } = await supabase.auth.getUser();

  context.locals.user = user;
  context.locals.supabase = supabase;

  const isProtected = PROTECTED.some((p) =>
    context.url.pathname.startsWith(p)
  );

  if (isProtected && !user) {
    return context.redirect(
      `/login?redirect=${encodeURIComponent(context.url.pathname)}`
    );
  }

  return next();
});
