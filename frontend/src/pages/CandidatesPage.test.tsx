import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { CandidatesPage } from '../pages/CandidatesPage';
import { backend } from '../services/backend';
import type { Candidate, Page, Position } from '../types';

// ── 测试辅助 ────────────────────────────────────────────────────────────

function makeCandidate(overrides: Partial<Candidate> = {}): Candidate {
  return {
    id: 'cand-1',
    name: '张三',
    source: 'manual',
    position_id: 'pos-1',
    experience_summary: null,
    skill_summary: null,
    expected_salary: null,
    expected_location: null,
    job_intention: null,
    notes: null,
    status: 'New',
    archived: false,
    created_at: '2026-11-02T13:00:00.000000Z',
    updated_at: '2026-11-02T13:00:00.000000Z',
    position_name: '前端工程师',
    message_count: 3,
    active_follow_up_count: 1,
    ...overrides,
  };
}

function makePosition(overrides: Partial<Position> = {}): Position {
  return {
    id: 'pos-1',
    name: '前端工程师',
    description: null,
    location: '北京',
    skill_requirements: 'React, TypeScript',
    experience_requirements: null,
    education_requirements: null,
    salary_min: null,
    salary_max: null,
    notes: null,
    status: 'Open',
    archived: false,
    created_at: '2026-11-01T00:00:00.000000Z',
    updated_at: '2026-11-01T00:00:00.000000Z',
    candidate_count: 2,
    ...overrides,
  };
}

function emptyPage<T>(): Page<T> {
  return { items: [], total: 0, offset: 0, limit: 50, has_more: false };
}

function pageOf<T>(items: T[]): Page<T> {
  return { items, total: items.length, offset: 0, limit: 50, has_more: false };
}

/** 渲染页面，默认 mock 候选人和岗位列表。 */
async function renderPage(
  candidates: Candidate[] = [makeCandidate()],
  positions: Position[] = [makePosition()],
) {
  vi.spyOn(backend.candidates, 'list').mockResolvedValue(pageOf(candidates));
  vi.spyOn(backend.positions, 'list').mockResolvedValue(pageOf(positions));
  render(
    <MemoryRouter>
      <CandidatesPage />
    </MemoryRouter>,
  );
  return screen.findByRole('heading', { name: '候选人' });
}

// ── 测试 ────────────────────────────────────────────────────────────────

describe('CandidatesPage — 列表展示', () => {
  it('渲染候选人列表表头与数据行', async () => {
    await renderPage([makeCandidate()]);
    expect(await screen.findByRole('heading', { name: '候选人' })).toBeInTheDocument();
    expect(screen.getByText('姓名')).toBeInTheDocument();
    expect(screen.getByText('岗位')).toBeInTheDocument();
    expect(screen.getByText('状态')).toBeInTheDocument();
    expect(screen.getByText('张三')).toBeInTheDocument();
    expect(screen.getByText('前端工程师')).toBeInTheDocument();
  });

  it('候选人无关联岗位时显示 em dash', async () => {
    await renderPage([makeCandidate({ position_name: null, position_id: null })]);
    expect(await screen.findByText('张三')).toBeInTheDocument();
    // 岗位列显示 —
    const cells = screen.getAllByText('—');
    expect(cells.length).toBeGreaterThanOrEqual(1);
  });

  it('没有候选人时显示空状态', async () => {
    vi.spyOn(backend.candidates, 'list').mockResolvedValue(emptyPage());
    vi.spyOn(backend.positions, 'list').mockResolvedValue(emptyPage());
    render(
      <MemoryRouter>
        <CandidatesPage />
      </MemoryRouter>,
    );
    expect(await screen.findByText('没有匹配的候选人')).toBeInTheDocument();
  });

  it('渲染操作按钮：编辑、归档、删除', async () => {
    await renderPage([makeCandidate()]);
    expect(await screen.findByRole('button', { name: '编辑' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '归档' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '删除' })).toBeInTheDocument();
  });

  it('已归档候选人显示取消归档按钮', async () => {
    await renderPage([makeCandidate({ archived: true })]);
    expect(await screen.findByRole('button', { name: '取消归档' })).toBeInTheDocument();
  });
});

describe('CandidatesPage — 搜索', () => {
  it('搜索框存在并可输入', async () => {
    await renderPage();
    const input = screen.getByPlaceholderText('搜索姓名、技能、地点…');
    expect(input).toBeInTheDocument();
    fireEvent.change(input, { target: { value: '李四' } });
    expect(input).toHaveValue('李四');
  });

  it('输入搜索词后以新参数重新调用 list', async () => {
    const mockList = vi
      .spyOn(backend.candidates, 'list')
      .mockResolvedValue(pageOf([makeCandidate()]));
    vi.spyOn(backend.positions, 'list').mockResolvedValue(pageOf([makePosition()]));
    render(
      <MemoryRouter>
        <CandidatesPage />
      </MemoryRouter>,
    );
    await screen.findByText('张三');
    const firstCallCount = mockList.mock.calls.length;
    const input = screen.getByPlaceholderText('搜索姓名、技能、地点…');
    fireEvent.change(input, { target: { value: '王五' } });
    await waitFor(() => {
      expect(mockList.mock.calls.length).toBeGreaterThan(firstCallCount);
    });
    // 最新一次调用应携带 search 参数
    const lastCall = mockList.mock.calls[mockList.mock.calls.length - 1][0];
    expect(lastCall).toMatchObject({ search: '王五' });
  });
});

describe('CandidatesPage — 删除确认', () => {
  it('点击删除弹出确认条，取消后不删除', async () => {
    await renderPage([makeCandidate()]);
    const deleteBtn = await screen.findByRole('button', { name: '删除' });
    fireEvent.click(deleteBtn);
    expect(
      screen.getByText(/确定删除「张三」？此操作不可恢复。/),
    ).toBeInTheDocument();
    // 点击取消
    const cancelBtn = screen.getByRole('button', { name: '取消' });
    fireEvent.click(cancelBtn);
    expect(
      screen.queryByText(/确定删除「张三」？/),
    ).not.toBeInTheDocument();
  });

  it('确认删除后调用 remove 并重新加载列表', async () => {
    const mockRemove = vi
      .spyOn(backend.candidates, 'remove')
      .mockResolvedValue(undefined);
    const mockList = vi
      .spyOn(backend.candidates, 'list')
      .mockResolvedValue(pageOf([makeCandidate()]));
    vi.spyOn(backend.positions, 'list').mockResolvedValue(pageOf([makePosition()]));
    render(
      <MemoryRouter>
        <CandidatesPage />
      </MemoryRouter>,
    );
    await screen.findByText('张三');
    const callsBefore = mockList.mock.calls.length;
    fireEvent.click(screen.getByRole('button', { name: '删除' }));
    fireEvent.click(screen.getByRole('button', { name: '确认删除' }));
    await waitFor(() => {
      expect(mockRemove).toHaveBeenCalledWith('cand-1');
    });
    await waitFor(() => {
      expect(mockList.mock.calls.length).toBeGreaterThan(callsBefore);
    });
  });
});

describe('CandidatesPage — 归档操作', () => {
  it('点击归档调用 update 切换 archived', async () => {
    const mockUpdate = vi
      .spyOn(backend.candidates, 'update')
      .mockResolvedValue(makeCandidate({ archived: true }));
    vi.spyOn(backend.candidates, 'list').mockResolvedValue(
      pageOf([makeCandidate({ archived: false })]),
    );
    vi.spyOn(backend.positions, 'list').mockResolvedValue(pageOf([makePosition()]));
    render(
      <MemoryRouter>
        <CandidatesPage />
      </MemoryRouter>,
    );
    await screen.findByText('张三');
    fireEvent.click(screen.getByRole('button', { name: '归档' }));
    await waitFor(() => {
      expect(mockUpdate).toHaveBeenCalledWith('cand-1', { archived: true });
    });
  });
});

describe('CandidatesPage — 新增候选人表单', () => {
  it('点击新增候选人按钮后显示表单', async () => {
    await renderPage();
    const toggleBtn = await screen.findByRole('button', { name: '新增候选人' });
    fireEvent.click(toggleBtn);
    expect(screen.getByText('新增候选人')).toBeInTheDocument();
    expect(screen.getByLabelText(/姓名 \/ 显示名称/)).toBeInTheDocument();
  });

  it('表单提交后调用 create', async () => {
    const mockCreate = vi
      .spyOn(backend.candidates, 'create')
      .mockResolvedValue(makeCandidate());
    vi.spyOn(backend.candidates, 'list').mockResolvedValue(pageOf([makeCandidate()]));
    vi.spyOn(backend.positions, 'list').mockResolvedValue(pageOf([makePosition()]));
    render(
      <MemoryRouter>
        <CandidatesPage />
      </MemoryRouter>,
    );
    await screen.findByText('张三');
    fireEvent.click(screen.getByRole('button', { name: '新增候选人' }));
    const nameInput = screen.getByLabelText(/姓名 \/ 显示名称/);
    fireEvent.change(nameInput, { target: { value: '王五' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => {
      expect(mockCreate).toHaveBeenCalledWith({ name: '王五', position_id: undefined });
    });
  });
});

describe('CandidatesPage — 岗位关联与解绑', () => {
  it('编辑时选择「暂不关联」后保存会显式提交 position_id: null', async () => {
    const mockUpdate = vi
      .spyOn(backend.candidates, 'update')
      .mockResolvedValue(makeCandidate({ position_id: null, position_name: null }));
    vi.spyOn(backend.candidates, 'list').mockResolvedValue(pageOf([makeCandidate()]));
    vi.spyOn(backend.positions, 'list').mockResolvedValue(pageOf([makePosition()]));
    render(
      <MemoryRouter>
        <CandidatesPage />
      </MemoryRouter>,
    );
    await screen.findByText('张三');
    fireEvent.click(screen.getByRole('button', { name: '编辑' }));
    const positionSelect = screen.getByLabelText('关联岗位');
    expect(positionSelect).toHaveValue('pos-1');
    fireEvent.change(positionSelect, { target: { value: '' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => {
      expect(mockUpdate).toHaveBeenCalledWith(
        'cand-1',
        expect.objectContaining({ position_id: null }),
      );
    });
  });

  it('编辑时改挂到另一个岗位提交新 position_id', async () => {
    const mockUpdate = vi
      .spyOn(backend.candidates, 'update')
      .mockResolvedValue(makeCandidate({ position_id: 'pos-2', position_name: '后端工程师' }));
    vi.spyOn(backend.candidates, 'list').mockResolvedValue(pageOf([makeCandidate()]));
    vi.spyOn(backend.positions, 'list').mockResolvedValue(
      pageOf([makePosition(), makePosition({ id: 'pos-2', name: '后端工程师' })]),
    );
    render(
      <MemoryRouter>
        <CandidatesPage />
      </MemoryRouter>,
    );
    await screen.findByText('张三');
    fireEvent.click(screen.getByRole('button', { name: '编辑' }));
    fireEvent.change(screen.getByLabelText('关联岗位'), { target: { value: 'pos-2' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => {
      expect(mockUpdate).toHaveBeenCalledWith(
        'cand-1',
        expect.objectContaining({ position_id: 'pos-2' }),
      );
    });
  });
});
