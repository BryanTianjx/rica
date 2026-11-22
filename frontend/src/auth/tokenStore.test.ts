import { describe, expect, it, beforeEach } from 'vitest';
import {
  clearTokens,
  getAccessToken,
  getRefreshToken,
  notifySessionExpired,
  onSessionExpired,
  setTokens,
} from './tokenStore';

describe('tokenStore', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('starts empty', () => {
    expect(getAccessToken()).toBeNull();
    expect(getRefreshToken()).toBeNull();
  });

  it('stores and reads a token pair', () => {
    setTokens('access-123', 'refresh-456');
    expect(getAccessToken()).toBe('access-123');
    expect(getRefreshToken()).toBe('refresh-456');
  });

  it('clears both tokens', () => {
    setTokens('access-123', 'refresh-456');
    clearTokens();
    expect(getAccessToken()).toBeNull();
    expect(getRefreshToken()).toBeNull();
  });

  it('persists across a reload (localStorage survives remount)', () => {
    setTokens('access-abc', 'refresh-def');
    // Simulate a page reload by reading straight from storage.
    expect(localStorage.getItem('ica.access_token')).toBe('access-abc');
    expect(localStorage.getItem('ica.refresh_token')).toBe('refresh-def');
  });

  it('notifies subscribers when the session expires', () => {
    const calls: string[] = [];
    onSessionExpired(() => calls.push('a'));
    onSessionExpired(() => calls.push('b'));
    notifySessionExpired();
    expect(calls).toEqual(['a', 'b']);
  });

  it('stops notifying after unsubscribe', () => {
    const calls: string[] = [];
    const unsubscribe = onSessionExpired(() => calls.push('x'));
    unsubscribe();
    notifySessionExpired();
    expect(calls).toEqual([]);
  });

  it('keeps notifying other subscribers when one throws', () => {
    const calls: string[] = [];
    onSessionExpired(() => {
      throw new Error('boom');
    });
    onSessionExpired(() => calls.push('alive'));
    expect(() => notifySessionExpired()).not.toThrow();
    expect(calls).toEqual(['alive']);
  });

  it('clearTokens itself does not trigger the expired notification', () => {
    const calls: string[] = [];
    onSessionExpired(() => calls.push('noop'));
    setTokens('a', 'b');
    clearTokens();
    expect(calls).toEqual([]);
  });
});
