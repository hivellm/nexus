/**
 * TableView — typed TanStack Table render of a CypherResponse.
 * Each row is the array-of-arrays the server returns; we synthesize
 * column accessors over the index so the renderer stays generic
 * across whatever shape the user's query projects.
 *
 * Sticky header, monospaced cells, a leading row-index column for
 * orientation when the user scrolls.
 */
import {
  flexRender,
  getCoreRowModel,
  useReactTable,
  type ColumnDef,
} from '@tanstack/react-table';
import { useMemo } from 'react';
import type { CypherResponse } from '../../types/api';

interface TableViewProps {
  result: CypherResponse | null;
}

interface RowShape {
  __index: number;
  values: unknown[];
}

function renderCell(value: unknown): string {
  if (value === null || value === undefined) return '—';
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean')
    return value.toString();
  return JSON.stringify(value);
}

export function TableView({ result }: TableViewProps) {
  const data: RowShape[] = useMemo(() => {
    if (!result) return [];
    return result.rows.map((row, i) => ({ __index: i, values: row }));
  }, [result]);

  const columns: ColumnDef<RowShape>[] = useMemo(() => {
    const cols: ColumnDef<RowShape>[] = [
      {
        id: '__index',
        header: '#',
        cell: (ctx) => <span className="row-index">{ctx.row.original.__index + 1}</span>,
        size: 48,
      },
    ];
    if (result) {
      result.columns.forEach((name, i) => {
        cols.push({
          id: name,
          header: name,
          cell: (ctx) => (
            <span className="mono">{renderCell(ctx.row.original.values[i])}</span>
          ),
        });
      });
    }
    return cols;
  }, [result]);

  const table = useReactTable({
    data,
    columns,
    getCoreRowModel: getCoreRowModel(),
  });

  if (!result) {
    return (
      <div className="results-empty">
        Run a query to see tabular results here.
      </div>
    );
  }

  return (
    <div className="results-table">
      <table>
        <thead>
          {table.getHeaderGroups().map((hg) => (
            <tr key={hg.id}>
              {hg.headers.map((h) => (
                <th key={h.id} style={{ width: h.column.getSize() }}>
                  {h.isPlaceholder
                    ? null
                    : flexRender(h.column.columnDef.header, h.getContext())}
                </th>
              ))}
            </tr>
          ))}
        </thead>
        <tbody>
          {table.getRowModel().rows.map((row) => (
            <tr key={row.id}>
              {row.getVisibleCells().map((cell) => (
                <td key={cell.id}>
                  {flexRender(cell.column.columnDef.cell, cell.getContext())}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
