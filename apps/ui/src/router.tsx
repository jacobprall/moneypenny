import type { QueryClient } from '@tanstack/react-query'
import { createRouter } from '@tanstack/react-router'
import { rootRoute } from '@/routes/__root'
import { ideasRoute } from '@/routes/ideas'
import { indexRoute } from '@/routes/index'
import { searchRoute } from '@/routes/search'
import { sessionRoute } from '@/routes/s.$sessionId'

const routeTree = rootRoute.addChildren([indexRoute, sessionRoute, ideasRoute, searchRoute])

function RouteError() {
  return (
    <div style={{ padding: 32, fontFamily: 'monospace' }}>
      <h2>Something went wrong</h2>
      <p>An error occurred loading this page. Try refreshing or navigating back.</p>
    </div>
  )
}

export function createAppRouter(queryClient: QueryClient) {
  return createRouter({
    routeTree,
    context: { queryClient },
    defaultErrorComponent: RouteError,
  })
}

declare module '@tanstack/react-router' {
  interface Register {
    router: ReturnType<typeof createAppRouter>
  }
}
