import { createBrowserClient, createServerClient, parseCookieHeader } from "@supabase/ssr";
import type { AstroCookies } from "astro";

const supabaseUrl = import.meta.env.PUBLIC_SUPABASE_URL;
const supabaseAnonKey = import.meta.env.PUBLIC_SUPABASE_ANON_KEY;

export function createSupabaseBrowser() {
  return createBrowserClient(supabaseUrl, supabaseAnonKey);
}

export function createSupabaseServer(
  cookies: AstroCookies,
  cookieHeader?: string
) {
  return createServerClient(supabaseUrl, supabaseAnonKey, {
    cookies: {
      getAll() {
        return parseCookieHeader(cookieHeader ?? "");
      },
      setAll(cookiesToSet) {
        cookiesToSet.forEach(({ name, value, options }) => {
          cookies.set(name, value, options);
        });
      },
    },
  });
}
