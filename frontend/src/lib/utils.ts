import { clsx, type ClassValue } from 'clsx'
import { twMerge } from 'tailwind-merge'

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs))
}

export function getPageNumbers(currentPage: number, totalPages: number): Array<number | '...'> {
  if (totalPages <= 0) return []
  const maxVisiblePages = 5
  const pageNumbers: Array<number | '...'> = []

  if (totalPages <= maxVisiblePages) {
    for (let i = 1; i <= totalPages; i += 1) {
      pageNumbers.push(i)
    }
    return pageNumbers
  }

  pageNumbers.push(1)

  if (currentPage <= 3) {
    for (let i = 2; i <= 4; i += 1) {
      pageNumbers.push(i)
    }
    pageNumbers.push('...', totalPages)
    return pageNumbers
  }

  if (currentPage >= totalPages - 2) {
    pageNumbers.push('...')
    for (let i = totalPages - 3; i < totalPages; i += 1) {
      pageNumbers.push(i)
    }
    pageNumbers.push(totalPages)
    return pageNumbers
  }

  pageNumbers.push('...', currentPage - 1, currentPage, currentPage + 1, '...', totalPages)
  return pageNumbers
}
