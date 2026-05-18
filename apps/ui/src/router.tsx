import type { QueryClient } from '@tanstack/react-query'
import { createRouter } from '@tanstack/react-router'
import { rootRoute } from '@/routes/__root'
import { ideasRoute } from '@/routes/ideas'
import { indexRoute } from '@/routes/index'
import { searchRoute } from '@/routes/search'
import { sessionRoute } from '@/routes/s.$sessionId'

const routeTree = rootRoute.addChildren([indexRoute, sessionRoute, ideasRoute, searchRoute])

export function createAppRouter(queryClient: QueryClient) {
  return createRouter({
    routeTree,
    context: { queryClient },
  })
}

declare module '@tanstack/react-router' {
  interface Register {
    router: ReturnType<typeof createAppRouter>
  }
}
