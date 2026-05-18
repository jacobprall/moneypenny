import { flexRender, type Table as TanTable } from '@tanstack/react-table'

export function ResultSection<T>(props: {
  title: string
  table: TanTable<T>
  onRowClick?: (row: T) => void
}) {
  return (
    <section>
      <h2 className="mb-2 font-mono text-xs font-semibold uppercase tracking-wide text-fg-dim">
        {props.title}
      </h2>
      <div className="border border-border bg-panel">
        <table className="w-full font-mono text-sm">
          <thead>
            {props.table.getHeaderGroups().map((hg) => (
              <tr key={hg.id} className="border-b border-border text-left">
                {hg.headers.map((h) => (
                  <th key={h.id} className="px-3 py-2 text-xs uppercase text-fg-dim">
                    {flexRender(h.column.columnDef.header, h.getContext())}
                  </th>
                ))}
              </tr>
            ))}
          </thead>
          <tbody>
            {props.table.getRowModel().rows.map((row) => (
              <tr
                key={row.id}
                className={
                  props.onRowClick
                    ? 'cursor-pointer border-b border-border hover:bg-panel-active'
                    : 'border-b border-border'
                }
                onClick={() => props.onRowClick?.(row.original)}
              >
                {row.getVisibleCells().map((cell) => (
                  <td key={cell.id} className="px-3 py-2">
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  )
}
