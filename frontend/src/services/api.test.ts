import { describe, it, expect } from 'vitest';
import { ApiError, describeApiError } from './api';

describe('describeApiError (F0-4 single error-render path)', () => {
  it('renders ApiError as code: message', () => {
    const err = new ApiError(409, { error: { code: 'business_rule_violation', message: '已完成', details: {} } }, 'fallback');
    expect(describeApiError(err, 'fallback')).toBe('business_rule_violation: 已完成');
  });

  it('renders a plain Error by its message', () => {
    expect(describeApiError(new Error('网络断了'), 'fallback')).toBe('网络断了');
  });

  it('uses the fallback for a non-Error throw', () => {
    expect(describeApiError('weird', '保存失败')).toBe('保存失败');
  });
});
