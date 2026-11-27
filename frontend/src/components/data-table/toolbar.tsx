import type * as React from 'react'
import { type Table } from '@tanstack/react-table'
import { X } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { DataTableFacetedFilter, type FacetedFilterOption } from './faceted-filter'
import { DataTableViewOptions } from './view-options'

export interface DataTableFilterConfig {
  columnId: string
  title: string
  options: ReadonlyArray<FacetedFilterOption>
}

interface DataTableToolbarProps<TData> {
  table: Table<TData>
  searchPlaceholder?: string
  searchKey?: string
  filters?: ReadonlyArray<DataTableFilterConfig>
  extraActions?: React.ReactNode
}

export function DataTableToolbar<TData>({
  table,
  searchPlaceholder,
  searchKey,
  filters = [],
  extraActions,
}: DataTableToolbarProps<TData>) {
  const { t } = useTranslation()
  const state = table.getState()
  const globalFilterStr = typeof state.globalFilter === 'string' ? state.globalFilter : ''
  const isFiltered = state.columnFilters.length > 0 || globalFilterStr.length > 0

  return (
    <div className="flex flex-wrap items-center justify-between gap-2">
      <div className="flex flex-1 flex-wrap items-center gap-2">
        {searchKey ? (
          <Input
            placeholder={searchPlaceholder ?? t('table.searchPlaceholder')}
            value={(table.getColumn(searchKey)?.getFilterValue() as string | undefined) ?? ''}
            onChange={(event) => {
              table.getColumn(searchKey)?.setFilterValue(event.target.value)
            }}
            className="h-8 w-[180px] lg:w-[260px]"
          />
        ) : (
          <Input
            placeholder={searchPlaceholder ?? t('table.searchPlaceholder')}
            value={globalFilterStr}
            onChange={(event) => {
              table.setGlobalFilter(event.target.value)
            }}
            className="h-8 w-[180px] lg:w-[260px]"
          />
        )}

        {filters.map((filter) => {
          const column = table.getColumn(filter.columnId)
          if (!column) return null
          return (
            <DataTableFacetedFilter
              key={filter.columnId}
              column={column}
              title={filter.title}
              options={filter.options}
            />
          )
        })}

        {isFiltered && (
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              table.resetColumnFilters()
              table.setGlobalFilter('')
            }}
            className="h-8 px-2 lg:px-3"
          >
            <span>{t('table.reset')}</span>
            <X className="size-4" />
          </Button>
        )}
      </div>

      <div className="flex items-center gap-2">
        {extraActions}
        <DataTableViewOptions table={table} />
      </div>
    </div>
  )
}
