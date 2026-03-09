import { createServerClient, parseCookieHeader } from '@supabase/ssr';

const supabaseUrl = "https://diplkmwyzrvuaptonfoq.supabase.co";
const supabaseAnonKey = "sb_publishable_e6FfwAGBY8HGgVq_Iw47qg_vnFXhZN1";
function createSupabaseServer(cookies, cookieHeader) {
  return createServerClient(supabaseUrl, supabaseAnonKey, {
    cookies: {
      getAll() {
        return parseCookieHeader(cookieHeader ?? "");
      },
      setAll(cookiesToSet) {
        cookiesToSet.forEach(({ name, value, options }) => {
          cookies.set(name, value, options);
        });
      }
    }
  });
}

export { createSupabaseServer as c };
