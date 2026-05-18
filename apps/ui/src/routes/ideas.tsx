import { createRoute } from '@tanstack/react-router'
import { IdeasTable } from '@/components/ideas/table'
import { rootRoute } from '@/routes/__root'

export const ideasRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/ideas',
  component: IdeasPage,
})

function IdeasPage() {
  return (
    <div className="h-full overflow-auto px-6 py-4">
      <IdeasTable />
    </div>
  )
}
