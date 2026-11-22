import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { PositionsPage } from '../pages/PositionsPage';
import { positionsApi } from '../services/endpoints';
import { DEFAULT_POSITION_STATUS, POSITION_STATUSES } from '../positionStatuses';
import type { Position, Page } from '../types';

// ── 测试辅助 ────────────────────────────────────────────────────────────

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
    candidate_count: 3,
    ...overrides,
  };
}

function pageOf<T>(items: T[]): Page<T> {
  return { items, total: items.length, offset: 0, limit: 50, has_more: false };
}

function emptyPage<T>(): Page<T> {
  return { items: [], total: 0, offset: 0, limit: 50, has_more: false };
}

/** 渲染 PositionsPage，默认 mock 岗位列表。 */
async function renderPage(positions: Position[] = [makePosition()]) {
  vi.spyOn(positionsApi, 'list').mockResolvedValue(pageOf(positions));
  render(
    <MemoryRouter>
      <PositionsPage />
    </MemoryRouter>,
  );
  return screen.findByRole('heading', { name: '岗位管理' });
}

// ── 测试 ────────────────────────────────────────────────────────────────

describe('PositionsPage — 列表展示', () => {
  it('渲染岗位列表表头与数据行', async () => {
    await renderPage([makePosition()]);
    expect(await screen.findByRole('heading', { name: '岗位管理' })).toBeInTheDocument();
    expect(screen.getByText('岗位')).toBeInTheDocument();
    expect(screen.getByText('地点')).toBeInTheDocument();
    expect(screen.getByText('技能要求')).toBeInTheDocument();
    expect(screen.getByText('状态')).toBeInTheDocument();
    expect(screen.getByText('候选人')).toBeInTheDocument();
    expect(screen.getByText('前端工程师')).toBeInTheDocument();
    expect(screen.getByText('北京')).toBeInTheDocument();
    expect(screen.getByText('React, TypeScript')).toBeInTheDocument();
    expect(screen.getByText('3')).toBeInTheDocument(); // candidate_count
  });

  it('岗位无地点和技能要求时显示 em dash', async () => {
    await renderPage([makePosition({ location: null, skill_requirements: null })]);
    expect(await screen.findByText('前端工程师')).toBeInTheDocument();
    const dashes = screen.getAllByText('—');
    expect(dashes.length).toBeGreaterThanOrEqual(2);
  });

  it('没有岗位时显示空状态', async () => {
    vi.spyOn(positionsApi, 'list').mockResolvedValue(emptyPage());
    render(
      <MemoryRouter>
        <PositionsPage />
      </MemoryRouter>,
    );
    expect(await screen.findByText('还没有岗位')).toBeInTheDocument();
  });

  it('渲染操作按钮：编辑、归档、删除', async () => {
    await renderPage();
    expect(await screen.findByRole('button', { name: '编辑' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '归档' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '删除' })).toBeInTheDocument();
  });

  it('已归档岗位显示取消归档按钮', async () => {
    await renderPage([makePosition({ archived: true })]);
    expect(await screen.findByRole('button', { name: '取消归档' })).toBeInTheDocument();
  });
});

describe('PositionsPage — 新增岗位表单', () => {
  it('点击新增岗位按钮后显示表单', async () => {
    await renderPage();
    const toggleBtn = await screen.findByRole('button', { name: '新增岗位' });
    fireEvent.click(toggleBtn);
    expect(screen.getByRole('heading', { name: '新增岗位' })).toBeInTheDocument();
    expect(screen.getByLabelText('岗位名称 *')).toBeInTheDocument();
    expect(screen.getByLabelText('工作地点')).toBeInTheDocument();
    expect(screen.getByLabelText('技能要求')).toBeInTheDocument();
  });

  it('岗位名称为空时保存按钮禁用', async () => {
    await renderPage();
    fireEvent.click(await screen.findByRole('button', { name: '新增岗位' }));
    const saveBtn = screen.getByRole('button', { name: '保存' });
    expect(saveBtn).toBeDisabled();
  });

  it('表单提交后调用 create', async () => {
    const mockCreate = vi
      .spyOn(positionsApi, 'create')
      .mockResolvedValue(makePosition());
    vi.spyOn(positionsApi, 'list').mockResolvedValue(pageOf([makePosition()]));
    render(
      <MemoryRouter>
        <PositionsPage />
      </MemoryRouter>,
    );
    await screen.findByText('前端工程师');
    fireEvent.click(screen.getByRole('button', { name: '新增岗位' }));
    const nameInput = screen.getByLabelText('岗位名称 *');
    fireEvent.change(nameInput, { target: { value: '后端工程师' } });
    const locationInput = screen.getByLabelText('工作地点');
    fireEvent.change(locationInput, { target: { value: '上海' } });
    const skillsInput = screen.getByLabelText('技能要求');
    fireEvent.change(skillsInput, { target: { value: 'Python, FastAPI' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => {
      expect(mockCreate).toHaveBeenCalledWith({
        name: '后端工程师',
        location: '上海',
        skill_requirements: 'Python, FastAPI',
      });
    });
    expect(await screen.findByText('岗位已创建。')).toBeInTheDocument();
  });
});

describe('PositionsPage — 编辑岗位', () => {
  it('点击编辑后打开预填的编辑表单', async () => {
    await renderPage();
    fireEvent.click(await screen.findByRole('button', { name: '编辑' }));
    // 表单式编辑：卡片标题切到「编辑岗位」，字段已预填，保存/取消挂在表单上。
    expect(screen.getByRole('heading', { name: '编辑岗位' })).toBeInTheDocument();
    expect(screen.getByLabelText('岗位名称 *')).toHaveValue('前端工程师');
    expect(screen.getByRole('button', { name: '保存' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '取消' })).toBeInTheDocument();
  });

  it('编辑修改名称后保存调用 update', async () => {
    const mockUpdate = vi
      .spyOn(positionsApi, 'update')
      .mockResolvedValue(makePosition());
    vi.spyOn(positionsApi, 'list').mockResolvedValue(pageOf([makePosition()]));
    render(
      <MemoryRouter>
        <PositionsPage />
      </MemoryRouter>,
    );
    await screen.findByText('前端工程师');
    fireEvent.click(screen.getByRole('button', { name: '编辑' }));
    const nameInput = screen.getByDisplayValue('前端工程师');
    fireEvent.change(nameInput, { target: { value: '高级前端工程师' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => {
      expect(mockUpdate).toHaveBeenCalledWith(
        'pos-1',
        expect.objectContaining({
          name: '高级前端工程师',
          status: 'Open',
        }),
      );
    });
  });

  it('点击取消退出编辑模式', async () => {
    await renderPage();
    fireEvent.click(await screen.findByRole('button', { name: '编辑' }));
    fireEvent.click(screen.getByRole('button', { name: '取消' }));
    // 退出编辑后恢复编辑按钮
    expect(screen.getByRole('button', { name: '编辑' })).toBeInTheDocument();
  });
});

describe('PositionsPage — 删除确认', () => {
  it('点击删除弹出确认条', async () => {
    await renderPage();
    fireEvent.click(await screen.findByRole('button', { name: '删除' }));
    expect(
      screen.getByText(/确定删除「前端工程师」？此操作不可恢复。/),
    ).toBeInTheDocument();
  });

  it('取消删除不调用 remove', async () => {
    const mockRemove = vi
      .spyOn(positionsApi, 'remove')
      .mockResolvedValue(undefined);
    await renderPage();
    fireEvent.click(await screen.findByRole('button', { name: '删除' }));
    fireEvent.click(screen.getByRole('button', { name: '取消' }));
    expect(mockRemove).not.toHaveBeenCalled();
  });

  it('确认删除调用 remove 并重新加载', async () => {
    const mockRemove = vi
      .spyOn(positionsApi, 'remove')
      .mockResolvedValue(undefined);
    const mockList = vi
      .spyOn(positionsApi, 'list')
      .mockResolvedValue(pageOf([makePosition()]));
    render(
      <MemoryRouter>
        <PositionsPage />
      </MemoryRouter>,
    );
    await screen.findByText('前端工程师');
    const callsBefore = mockList.mock.calls.length;
    fireEvent.click(screen.getByRole('button', { name: '删除' }));
    fireEvent.click(screen.getByRole('button', { name: '确认删除' }));
    await waitFor(() => expect(mockRemove).toHaveBeenCalledWith('pos-1'));
    await waitFor(() =>
      expect(mockList.mock.calls.length).toBeGreaterThan(callsBefore),
    );
  });
});

describe('PositionsPage — 归档', () => {
  it('点击归档调用 update 切换 archived', async () => {
    const mockUpdate = vi
      .spyOn(positionsApi, 'update')
      .mockResolvedValue(makePosition({ archived: true }));
    vi.spyOn(positionsApi, 'list').mockResolvedValue(
      pageOf([makePosition({ archived: false })]),
    );
    render(
      <MemoryRouter>
        <PositionsPage />
      </MemoryRouter>,
    );
    await screen.findByText('前端工程师');
    fireEvent.click(screen.getByRole('button', { name: '归档' }));
    await waitFor(() => {
      expect(mockUpdate).toHaveBeenCalledWith('pos-1', { archived: true });
    });
  });
});

// ── PRD §5.3: 岗位字段完整性 ─────────────────────────────────────────────

describe('PositionsPage — 岗位字段完整性', () => {
  it('新增表单暴露岗位描述/经验/学历/薪资/备注等字段', async () => {
    await renderPage();
    fireEvent.click(await screen.findByRole('button', { name: '新增岗位' }));
    expect(screen.getByLabelText('岗位名称 *')).toBeInTheDocument();
    expect(screen.getByLabelText('工作地点')).toBeInTheDocument();
    expect(screen.getByLabelText('岗位状态')).toBeInTheDocument();
    expect(screen.getByLabelText('岗位描述')).toBeInTheDocument();
    expect(screen.getByLabelText('技能要求')).toBeInTheDocument();
    expect(screen.getByLabelText('经验要求')).toBeInTheDocument();
    expect(screen.getByLabelText('学历要求')).toBeInTheDocument();
    expect(screen.getByLabelText('最低薪资')).toBeInTheDocument();
    expect(screen.getByLabelText('最高薪资')).toBeInTheDocument();
    expect(screen.getByLabelText('备注')).toBeInTheDocument();
  });

  it('岗位状态下拉选项来自 positionStatuses 单一词表', async () => {
    await renderPage();
    fireEvent.click(await screen.findByRole('button', { name: '新增岗位' }));
    const select = screen.getByLabelText('岗位状态') as HTMLSelectElement;
    // C8：词表只有 ../positionStatuses 一个来源，页面不再自带字面量数组。
    expect(Array.from(select.options).map((option) => option.value)).toEqual(
      POSITION_STATUSES,
    );
    expect(select).toHaveValue(DEFAULT_POSITION_STATUS);
  });

  it('创建时把全部扩展字段提交到 API', async () => {
    const mockCreate = vi
      .spyOn(positionsApi, 'create')
      .mockResolvedValue(makePosition());
    await renderPage();
    fireEvent.click(await screen.findByRole('button', { name: '新增岗位' }));
    fireEvent.change(screen.getByLabelText('岗位名称 *'), {
      target: { value: '后端工程师' },
    });
    fireEvent.change(screen.getByLabelText('工作地点'), { target: { value: '上海' } });
    fireEvent.change(screen.getByLabelText('岗位描述'), {
      target: { value: '负责核心服务' },
    });
    fireEvent.change(screen.getByLabelText('技能要求'), {
      target: { value: 'Python, FastAPI' },
    });
    fireEvent.change(screen.getByLabelText('经验要求'), { target: { value: '3 年以上' } });
    fireEvent.change(screen.getByLabelText('学历要求'), { target: { value: '本科' } });
    fireEvent.change(screen.getByLabelText('最低薪资'), { target: { value: '20000' } });
    fireEvent.change(screen.getByLabelText('最高薪资'), { target: { value: '30000' } });
    fireEvent.change(screen.getByLabelText('备注'), { target: { value: '急招' } });
    fireEvent.change(screen.getByLabelText('岗位状态'), { target: { value: 'Draft' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => {
      expect(mockCreate).toHaveBeenCalledWith({
        name: '后端工程师',
        description: '负责核心服务',
        location: '上海',
        skill_requirements: 'Python, FastAPI',
        experience_requirements: '3 年以上',
        education_requirements: '本科',
        salary_min: 20000,
        salary_max: 30000,
        notes: '急招',
        status: 'Draft',
      });
    });
  });

  it('薪资下限高于上限时阻止提交并提示', async () => {
    const mockCreate = vi.spyOn(positionsApi, 'create');
    await renderPage();
    fireEvent.click(await screen.findByRole('button', { name: '新增岗位' }));
    fireEvent.change(screen.getByLabelText('岗位名称 *'), { target: { value: '测试岗' } });
    fireEvent.change(screen.getByLabelText('最低薪资'), { target: { value: '30000' } });
    fireEvent.change(screen.getByLabelText('最高薪资'), { target: { value: '20000' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    expect(await screen.findByText('最低薪资不能高于最高薪资。')).toBeInTheDocument();
    expect(mockCreate).not.toHaveBeenCalled();
  });

  it('负薪资被输入约束拦截，不会提交', async () => {
    const mockCreate = vi.spyOn(positionsApi, 'create');
    await renderPage();
    fireEvent.click(await screen.findByRole('button', { name: '新增岗位' }));
    fireEvent.change(screen.getByLabelText('岗位名称 *'), { target: { value: '测试岗' } });
    const minInput = screen.getByLabelText('最低薪资') as HTMLInputElement;
    fireEvent.change(minInput, { target: { value: '-1' } });
    // 输入层约束：number + min=0，浏览器在提交前就拒绝该值。
    expect(minInput.checkValidity()).toBe(false);
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => expect(mockCreate).not.toHaveBeenCalled());
  });
});

describe('PositionsPage — 编辑完整字段', () => {
  const detailed = makePosition({
    description: '旧描述',
    experience_requirements: '2 年',
    education_requirements: '大专',
    salary_min: 10000,
    salary_max: 20000,
    notes: '旧备注',
  });

  it('编辑表单预填已有字段并可提交修改', async () => {
    const mockUpdate = vi.spyOn(positionsApi, 'update').mockResolvedValue(detailed);
    vi.spyOn(positionsApi, 'list').mockResolvedValue(pageOf([detailed]));
    render(
      <MemoryRouter>
        <PositionsPage />
      </MemoryRouter>,
    );
    await screen.findByText('前端工程师');
    fireEvent.click(screen.getByRole('button', { name: '编辑' }));
    expect(screen.getByLabelText('岗位描述')).toHaveValue('旧描述');
    expect(screen.getByLabelText('经验要求')).toHaveValue('2 年');
    expect(screen.getByLabelText('学历要求')).toHaveValue('大专');
    expect(screen.getByLabelText('最低薪资')).toHaveValue(10000);
    expect(screen.getByLabelText('最高薪资')).toHaveValue(20000);
    expect(screen.getByLabelText('备注')).toHaveValue('旧备注');

    fireEvent.change(screen.getByLabelText('岗位描述'), { target: { value: '新描述' } });
    fireEvent.change(screen.getByLabelText('最低薪资'), { target: { value: '' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => {
      expect(mockUpdate).toHaveBeenCalledWith(
        'pos-1',
        expect.objectContaining({
          description: '新描述',
          salary_min: null,
          salary_max: 20000,
          status: 'Open',
        }),
      );
    });
    expect(await screen.findByText('岗位已更新。')).toBeInTheDocument();
  });

  it('清空的可选文本字段以 null 提交（真正清除）', async () => {
    const mockUpdate = vi.spyOn(positionsApi, 'update').mockResolvedValue(detailed);
    vi.spyOn(positionsApi, 'list').mockResolvedValue(pageOf([detailed]));
    render(
      <MemoryRouter>
        <PositionsPage />
      </MemoryRouter>,
    );
    await screen.findByText('前端工程师');
    fireEvent.click(screen.getByRole('button', { name: '编辑' }));
    fireEvent.change(screen.getByLabelText('备注'), { target: { value: '' } });
    fireEvent.change(screen.getByLabelText('经验要求'), { target: { value: '' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => {
      expect(mockUpdate).toHaveBeenCalledWith(
        'pos-1',
        expect.objectContaining({ notes: null, experience_requirements: null }),
      );
    });
  });
});

// ── 搜索与归档过滤 ──────────────────────────────────────────────────────

describe('PositionsPage — 搜索与归档过滤', () => {
  it('输入搜索词后以 search 参数重新调用 list', async () => {
    const mockList = vi
      .spyOn(positionsApi, 'list')
      .mockResolvedValue(pageOf([makePosition()]));
    render(
      <MemoryRouter>
        <PositionsPage />
      </MemoryRouter>,
    );
    await screen.findByText('前端工程师');
    const before = mockList.mock.calls.length;
    fireEvent.change(screen.getByLabelText('搜索岗位'), { target: { value: '前端' } });
    await waitFor(() => expect(mockList.mock.calls.length).toBeGreaterThan(before));
    const lastArgs = mockList.mock.calls[mockList.mock.calls.length - 1][0];
    expect(lastArgs).toMatchObject({ search: '前端' });
  });

  it('切换显示已归档后以 include_archived 参数重新调用 list', async () => {
    const mockList = vi
      .spyOn(positionsApi, 'list')
      .mockResolvedValue(pageOf([makePosition()]));
    render(
      <MemoryRouter>
        <PositionsPage />
      </MemoryRouter>,
    );
    await screen.findByText('前端工程师');
    const toggle = screen.getByRole('button', { name: '显示已归档' });
    expect(toggle).toHaveAttribute('aria-pressed', 'false');
    fireEvent.click(toggle);
    await waitFor(() => {
      const lastArgs = mockList.mock.calls[mockList.mock.calls.length - 1][0];
      expect(lastArgs).toMatchObject({ include_archived: true });
    });
    expect(screen.getByRole('button', { name: '隐藏已归档' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
  });

  it('已归档岗位可以取消归档', async () => {
    const mockUpdate = vi
      .spyOn(positionsApi, 'update')
      .mockResolvedValue(makePosition({ archived: false }));
    vi.spyOn(positionsApi, 'list').mockResolvedValue(
      pageOf([makePosition({ archived: true })]),
    );
    render(
      <MemoryRouter>
        <PositionsPage />
      </MemoryRouter>,
    );
    await screen.findByText('前端工程师');
    fireEvent.click(screen.getByRole('button', { name: '取消归档' }));
    await waitFor(() => {
      expect(mockUpdate).toHaveBeenCalledWith('pos-1', { archived: false });
    });
  });
});
