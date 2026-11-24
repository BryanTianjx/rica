/** C6/C7: shared hooks must be imported by tests and have real call sites. */
import { describe, expect, it, vi } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { useAsync, useMutation } from './useAsync';

describe('useAsync', () => {
  it('loads data and exposes reload', async () => {
    const loader = vi.fn().mockResolvedValue([1, 2]);
    const { result } = renderHook(() => useAsync(loader, []));
    await waitFor(() => expect(result.current.data).toEqual([1, 2]));
    expect(result.current.loading).toBe(false);
    result.current.reload();
    await waitFor(() => expect(loader).toHaveBeenCalledTimes(2));
  });
});

describe('useMutation', () => {
  it('runs an action and tracks loading/error', async () => {
    const action = vi.fn().mockResolvedValue('ok');
    const { result } = renderHook(() => useMutation(action));
    let value: unknown = null;
    await waitFor(async () => { value = await result.current.run(); });
    expect(value).toBe('ok');
    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBeNull();
  });
});
