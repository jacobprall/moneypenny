import { createRoute } from '@tanstack/react-router'
import { SessionView } from '@/components/session/view'
import { queryKeys } from '@/lib/query-keys'
import { api } from '@/lib/rpc'
import { rootRoute } from '@/routes/__root'

export const sessionRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/s/$sessionId',
  component: SessionPage,
  loader: async ({ context, params }) => {
    await context.queryClient.ensureQueryData({
      queryKey: queryKeys.session(params.sessionId),
      queryFn: () => api.sessions.get(params.sessionId),
    })
    return {}
  },
})

function SessionPage() {
  const { sessionId } = sessionRoute.useParams()
  return (
    <div className="flex h-full flex-col">
      <SessionView sessionId={sessionId} />
    </div>
  )
}
