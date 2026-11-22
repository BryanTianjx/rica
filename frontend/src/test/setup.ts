import '@testing-library/jest-dom/vitest';
import { vi } from 'vitest';

// Globally available helpers (describe, it, expect, beforeEach, etc.) are
// auto-injected by Vitest's `globals: true` option.

// Reset all mocks between tests.
beforeEach(() => {
  vi.clearAllMocks();
});
