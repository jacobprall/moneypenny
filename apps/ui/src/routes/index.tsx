import { createRoute } from '@tanstack/react-router'
import { ActivityFeed } from '@/components/overview/activity-feed'
import { SessionsTable } from '@/components/overview/sessions-table'
import { StatTiles } from '@/components/overview/stat-tiles'
import { useGlobalEvents } from '@/hooks/use-global-events'
import { rootRoute } from '@/routes/__root'

export const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: 'index',
  path: '/',
  component: OverviewPage,
})

function OverviewPage() {
  const global = useGlobalEvents()
  return (
    <div className="grid h-full gap-6 overflow-auto px-6 py-4">
      <StatTiles />
      <SessionsTable />
      <ActivityFeed events={global.recent} />
    </div>
  )
}
