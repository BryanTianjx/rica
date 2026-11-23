import { describe, expect, it } from 'vitest';
import { crumbsFor } from './routeMeta';

describe('routeMeta', () => {
  it('maps the 10 existing routes without adding any', () => {
    expect(crumbsFor('/').map((c) => c.label)).toEqual(['工作台']);
    expect(crumbsFor('/candidates/abc').map((c) => c.label)).toEqual([
      '工作台', '候选人', '候选人详情',
    ]);
  });

  it('falls back to a not-found crumb for unknown paths', () => {
    expect(crumbsFor('/unknown').map((c) => c.label)).toEqual(['工作台', '页面不存在']);
  });
});
