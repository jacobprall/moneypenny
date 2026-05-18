import { createRoute } from '@tanstack/react-router'
import { SearchResults } from '@/components/search/results'
import { rootRoute } from '@/routes/__root'

export const searchRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/search',
  component: SearchPage,
})

function SearchPage() {
  return (
    <div className="h-full overflow-auto px-6 py-4">
      <SearchResults />
    </div>
  )
}
