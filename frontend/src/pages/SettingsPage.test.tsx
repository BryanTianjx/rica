import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { SettingsPage } from '../pages/SettingsPage';
import { ApiError } from '../services/backend';
import { backend } from '../services/backend';
import type {
  SettingsInfo,
  BackupRecord,
  FollowUpPolicy,
  AiEgressDisclosure,
  AiModel,
  AiModelsResponse,
  AuditEvent,
} from '../types';

// ── 测试辅助 ────────────────────────────────────────────────────────────

function makeSettings(overrides: Partial<SettingsInfo> = {}): SettingsInfo {
  return {
    app_name: 'ICA',
    ai_enabled: true,
    ai_provider: 'openai',
    ai_model: 'gpt-4o',
    ai_base_url: 'https://api.openai.com/v1',
    database_url: 'sqlite:///ica.db',
    backup_dir: '/backups',
    backup_keep_count: 10,
    ...overrides,
  };
}

function makePolicy(overrides: Partial<FollowUpPolicy> = {}): FollowUpPolicy {
  return {
    id: 'policy-1',
    name: 'default',
    first_follow_up_delay_hours: 24,
    max_active_follow_ups: 2,
    min_interval_hours: 48,
    task_validity_days: 7,
    is_active: true,
    ...overrides,
  };
}

function makeBackup(overrides: Partial<BackupRecord> = {}): BackupRecord {
  return {
    name: 'ica-20261102.db',
    path: '/backups/ica-20261102.db',
    size_bytes: 4096,
    created_at: '2026-11-02T13:00:00.000000Z',
    ...overrides,
  };
}


function makeEgress(overrides: Partial<AiEgressDisclosure> = {}): AiEgressDisclosure {
  return {
    enabled: true,
    consent_granted: true,
    provider: 'openai',
    model: 'gpt-4o',
    base_url: 'https://api.openai.com/v1',
    masked_fields: ['name', 'phone', 'email', 'id_number', 'card_number'],
    include_candidate_name: false,
    notice: '生成草稿时，只有脱敏后的候选人上下文会发送至所配置的第三方 AI 供应商。',
    ...overrides,
  };
}


/** 渲染 SettingsPage，默认 mock 三个数据源。 */
function makeAuditEvent(overrides: Partial<AuditEvent> = {}): AuditEvent {
  return {
    id: 'audit-1',
    event_type: 'candidate.updated',
    entity_type: 'candidate',
    entity_id: 'cand-1abcdef',
    actor: 'local',
    summary: 'Updated a candidate',
    metadata: {},
    created_at: '2026-11-16T00:00:00.000000Z',
    ...overrides,
  };
}

/** 一个已配置模型的默认夹具（数据库来源、已启用、默认、Key 已配置）。 */
function makeModel(overrides: Partial<AiModel> = {}): AiModel {
  return {
    model_id: 'gpt-4o',
    display_name: 'GPT-4o',
    provider_type: 'openai_compatible',
    base_url: 'https://api.openai.com/v1',
    api_key_env: 'AI_API_KEY',
    quality_tier: 'standard',
    input_cost_per_1k_tokens: 0,
    output_cost_per_1k_tokens: 0,
    max_output_tokens: 1500,
    max_input_tokens: 8000,
    timeout: 60,
    temperature: 0.4,
    enabled: true,
    is_default: true,
    sort_order: 0,
    api_key_configured: true,
    source: 'database',
    ...overrides,
  };
}

async function renderPage(
  settings: SettingsInfo = makeSettings(),
  policy: FollowUpPolicy = makePolicy(),
  backups: BackupRecord[] = [makeBackup()],
  egress: AiEgressDisclosure = makeEgress(),
  auditEvents: AuditEvent[] = [],
) {
  vi.spyOn(backend.settings, 'get').mockResolvedValue(settings);
  vi.spyOn(backend.settings, 'getAiEgress').mockResolvedValue(egress);
  vi.spyOn(backend.settings, 'setAiEgressConsent').mockResolvedValue(egress);
  vi.spyOn(backend.followUps, 'getPolicy').mockResolvedValue(policy);
  vi.spyOn(backend.followUps, 'updatePolicy').mockResolvedValue(policy);
  vi.spyOn(backend.settings, 'listBackups').mockResolvedValue(backups);
  vi.spyOn(backend.ai, 'listModels').mockResolvedValue({ models: [], routing: {} });
  const auditMock = vi.spyOn(backend.settings, 'listAuditEvents').mockResolvedValue(auditEvents);
  vi.spyOn(backend.settings, 'createBackup').mockResolvedValue(makeBackup());
  vi.spyOn(backend.settings, 'restore').mockResolvedValue({
    restored_from: 'ica-20261102.db',
    database: 'ica.db',
    previous_copy: 'ica-pre-restore.db',
    message: 'ok',
  });
  render(
    <MemoryRouter>
      <SettingsPage />
    </MemoryRouter>,
  );
  await screen.findByRole('heading', { name: '系统设置' });
  return { auditMock };
}

// ── 测试 ────────────────────────────────────────────────────────────────

describe('SettingsPage — AI 接入展示', () => {
  it('AI 已启用时显示"已启用"徽章及 provider / 模型 / 端点', async () => {
    await renderPage(makeSettings({ ai_enabled: true }));
    expect(await screen.findByText('已启用')).toBeInTheDocument();
    // The privacy card repeats provider / model / endpoint on purpose, so at
    // least one occurrence is what this test pins.
    expect(screen.getAllByText('openai').length).toBeGreaterThan(0);
    expect(screen.getAllByText('gpt-4o').length).toBeGreaterThan(0);
    expect(screen.getAllByText('https://api.openai.com/v1').length).toBeGreaterThan(0);
  });

  it('AI 未配置时显示"未配置"徽章与警告框', async () => {
    await renderPage(makeSettings({ ai_enabled: false }));
    expect(await screen.findByText('未配置')).toBeInTheDocument();
    expect(
      screen.getByText(/未配置 AI API Key/),
    ).toBeInTheDocument();
  });
});

describe('SettingsPage — 跟进策略', () => {
  it('展示策略四个字段并回填当前值', async () => {
    await renderPage();
    const delayInput = await screen.findByLabelText('首次跟进等待（小时）');
    expect(delayInput).toHaveValue(24);
    expect(screen.getByLabelText('最大主动跟进次数')).toHaveValue(2);
    expect(screen.getByLabelText('两次跟进最小间隔（小时）')).toHaveValue(48);
    expect(screen.getByLabelText('任务有效期（天）')).toHaveValue(7);
  });

  it('未修改策略时保存按钮禁用', async () => {
    await renderPage();
    const saveBtn = await screen.findByRole('button', { name: '保存策略' });
    expect(saveBtn).toBeDisabled();
  });

  it('修改策略后保存按钮启用，点击后调用 updatePolicy', async () => {
    const mockUpdate = vi
      .spyOn(backend.followUps, 'updatePolicy')
      .mockResolvedValue(makePolicy());
    await renderPage();
    const delayInput = await screen.findByLabelText('首次跟进等待（小时）');
    fireEvent.change(delayInput, { target: { value: '12' } });
    const saveBtn = screen.getByRole('button', { name: '保存策略' });
    expect(saveBtn).not.toBeDisabled();
    fireEvent.click(saveBtn);
    await waitFor(() => {
      expect(mockUpdate).toHaveBeenCalledWith(
        expect.objectContaining({ first_follow_up_delay_hours: 12 }),
      );
    });
    expect(await screen.findByText('跟进策略已更新。')).toBeInTheDocument();
  });
});

describe('SettingsPage — 本地备份', () => {
  it('展示已有备份列表', async () => {
    await renderPage();
    expect(await screen.findByText('ica-20261102.db')).toBeInTheDocument();
    expect(screen.getByText(/4096 字节/)).toBeInTheDocument();
  });

  it('没有备份时显示空状态', async () => {
    vi.spyOn(backend.settings, 'get').mockResolvedValue(makeSettings());
    vi.spyOn(backend.followUps, 'getPolicy').mockResolvedValue(makePolicy());
    vi.spyOn(backend.settings, 'listBackups').mockResolvedValue([]);
    render(
      <MemoryRouter>
        <SettingsPage />
      </MemoryRouter>,
    );
    expect(await screen.findByText('还没有备份')).toBeInTheDocument();
  });

  it('点击立即备份调用 createBackup 并显示通知', async () => {
    vi.spyOn(backend.settings, 'get').mockResolvedValue(makeSettings());
    vi.spyOn(backend.followUps, 'getPolicy').mockResolvedValue(makePolicy());
    vi.spyOn(backend.settings, 'listBackups').mockResolvedValue([makeBackup()]);
    const mockCreate = vi
      .spyOn(backend.settings, 'createBackup')
      .mockResolvedValue(makeBackup({ name: 'ica-20261103.db', size_bytes: 8192 }));
    render(
      <MemoryRouter>
        <SettingsPage />
      </MemoryRouter>,
    );
    const backupBtn = await screen.findByRole('button', { name: '立即备份' });
    fireEvent.click(backupBtn);
    await waitFor(() => expect(mockCreate).toHaveBeenCalled());
    expect(
      await screen.findByText(/已创建备份 ica-20261103.db/),
    ).toBeInTheDocument();
  });
});

describe('SettingsPage — 恢复确认', () => {
  it('点击恢复此备份弹出确认条，取消后不恢复', async () => {
    await renderPage();
    const restoreBtn = await screen.findByRole('button', { name: '恢复此备份' });
    fireEvent.click(restoreBtn);
    // 确认条文案被 <strong> 分隔，用 confirm-bar 容器断言
    const confirmBar = screen.getByText((_, node) => {
      return !!node && node.classList.contains('confirm-bar');
    });
    expect(confirmBar).toHaveTextContent('ica-20261102.db');
    expect(confirmBar).toHaveTextContent('恢复');
    fireEvent.click(screen.getByRole('button', { name: '取消' }));
    expect(confirmBar).not.toBeInTheDocument();
  });

  it('确认恢复后调用 backend.settings.restore 并显示通知', async () => {
    const mockRestore = vi.spyOn(backend.settings, 'restore').mockResolvedValue({
      restored_from: 'ica-20261102.db',
      database: 'ica.db',
      previous_copy: 'ica-pre-restore.db',
      message: 'ok',
    });
    await renderPage();
    const restoreBtn = await screen.findByRole('button', { name: '恢复此备份' });
    fireEvent.click(restoreBtn);
    fireEvent.click(screen.getByRole('button', { name: '确认恢复' }));
    await waitFor(() =>
      expect(mockRestore).toHaveBeenCalledWith('ica-20261102.db'),
    );
    expect(
      await screen.findByText(/已从 ica-20261102.db 恢复数据库/),
    ).toBeInTheDocument();
  });
});

describe('SettingsPage — 隐私与 AI 出网（PRIV-07）', () => {
  it('展示供应商、模型、端点、脱敏项与姓名处理方式', async () => {
    await renderPage();

    expect(
      await screen.findByRole('heading', { name: '隐私与 AI 出网' }),
    ).toBeInTheDocument();
    // 姓名这一项必须明确写出以占位符代替，否则用户会以为姓名也会出网。
    expect(screen.getByText(/姓名（以 \[NAME\] 占位）/)).toBeInTheDocument();
    expect(screen.getByText('以 [NAME] 占位，不发送')).toBeInTheDocument();
    expect(screen.getByText('允许出网')).toBeInTheDocument();
  });

  it('姓名被显式开启时如实显示随上下文发送', async () => {
    await renderPage(
      makeSettings(),
      makePolicy(),
      [makeBackup()],
      makeEgress({ include_candidate_name: true }),
    );

    expect(
      await screen.findByText(/随上下文发送（已显式开启 AI_INCLUDE_CANDIDATE_NAME）/),
    ).toBeInTheDocument();
  });

  it('点击关闭会调用后端并提示生成将被拒绝', async () => {
    await renderPage();

    fireEvent.click(await screen.findByRole('button', { name: '关闭 AI 出网' }));

    await waitFor(() => {
      expect(vi.mocked(backend.settings.setAiEgressConsent)).toHaveBeenCalledWith(false);
    });
    expect(
      await screen.findByText(/AI 出网已关闭，草稿生成会被拒绝/),
    ).toBeInTheDocument();
  });
});

// ── G409-GAP: 审计记录查看（OFF-017 的 UI 面） ──────────────────────────

describe('SettingsPage — 审计记录', () => {
  it('渲染审计事件表（时间/事件类型/对象/操作者/摘要）', async () => {
    await renderPage(
      makeSettings(),
      makePolicy(),
      [makeBackup()],
      makeEgress(),
      [
        makeAuditEvent(),
        makeAuditEvent({
          id: 'audit-2',
          event_type: 'message.imported',
          entity_type: 'message',
          entity_id: 'msg-9abcdef',
          summary: 'Imported 2 messages',
        }),
      ],
    );
    expect(await screen.findByRole('heading', { name: '审计记录' })).toBeInTheDocument();
    expect(screen.getByText('candidate.updated')).toBeInTheDocument();
    expect(screen.getByText('message.imported')).toBeInTheDocument();
    expect(screen.getAllByText('Updated a candidate').length).toBeGreaterThan(0);
    // 对象列显示 entity_type + entity_id 前 8 位
    expect(screen.getByText(/cand-1ab/)).toBeInTheDocument();
  });

  it('无记录时显示空状态', async () => {
    await renderPage();
    expect(await screen.findByText('没有匹配的审计记录')).toBeInTheDocument();
  });

  it('事件类型筛选改变时带 event_type 参数重新查询', async () => {
    const { auditMock } = await renderPage(
      makeSettings(),
      makePolicy(),
      [makeBackup()],
      makeEgress(),
      [makeAuditEvent()],
    );
    await screen.findByText('candidate.updated');

    fireEvent.change(screen.getByLabelText('事件类型筛选'), {
      target: { value: 'candidate.updated' },
    });
    await waitFor(() => {
      expect(auditMock).toHaveBeenLastCalledWith(
        expect.objectContaining({ event_type: 'candidate.updated' }),
      );
    });
    expect(await screen.findByText('candidate.updated')).toBeInTheDocument();
  });
});

// ── AI key 桌面端配置（G409-GAP 后续） ──────────────────────────────────

describe('SettingsPage — AI Key 配置', () => {
  async function openAiCard() {
    await renderPage();
    await screen.findByRole('heading', { name: 'AI 接入' });
    return screen.getByLabelText('API Key');
  }

  it('保存密钥调用 setAiKey 并以脱敏尾号反馈，输入框清空', async () => {
    const mockSet = vi
      .spyOn(backend.settings, 'setAiKey')
      .mockResolvedValue({ ai_enabled: true, key_masked: '…abcd' });
    const input = await openAiCard();
    fireEvent.change(input, { target: { value: 'sk-glm-live-9f8eabcd' } });
    fireEvent.click(screen.getByRole('button', { name: '保存密钥' }));

    await waitFor(() => {
      expect(mockSet).toHaveBeenCalledWith('sk-glm-live-9f8eabcd');
    });
    expect(await screen.findByText(/尾号 …abcd/)).toBeInTheDocument();
    // 完整密钥不出现在任何提示文案里
    expect(screen.queryByText(/sk-glm-live-9f8eabcd/)).not.toBeInTheDocument();
    expect((input as HTMLInputElement).value).toBe('');
  });

  it('清除密钥调用 clearAiKey 并停用 AI', async () => {
    const mockClear = vi
      .spyOn(backend.settings, 'clearAiKey')
      .mockResolvedValue({ ai_enabled: false, key_masked: null });
    const input = await openAiCard();
    fireEvent.click(screen.getByRole('button', { name: '清除密钥' }));

    await waitFor(() => {
      expect(mockClear).toHaveBeenCalled();
    });
    expect(await screen.findByText(/API Key 已清除/)).toBeInTheDocument();
    expect((input as HTMLInputElement).value).toBe('');
  });

  it('空输入时保存按钮禁用（不发起请求）', async () => {
    const mockSet = vi.spyOn(backend.settings, 'setAiKey');
    await openAiCard();
    expect(screen.getByRole('button', { name: '保存密钥' })).toBeDisabled();
    expect(mockSet).not.toHaveBeenCalled();
  });
});

// ── AI 自配置：可编辑 Provider / 端点 / 模型 + 模型管理（AC-F1..F5） ─────

/**
 * 渲染设置页，并让 `backend.ai.listModels` 依次返回给定响应（最后一次重复使用）。
 * 这样既能断言「保存后显示后端返回的生效值」，也能断言「操作后真的重新拉取列表」。
 */
async function renderAiPage(
  responses: AiModelsResponse[],
  settings: SettingsInfo = makeSettings(),
) {
  let call = 0;
  const listMock = vi.spyOn(backend.ai, 'listModels').mockImplementation(async () => {
    const response = responses[Math.min(call, responses.length - 1)];
    call += 1;
    return response;
  });
  vi.spyOn(backend.settings, 'get').mockResolvedValue(settings);
  vi.spyOn(backend.settings, 'getAiEgress').mockResolvedValue(makeEgress());
  vi.spyOn(backend.followUps, 'getPolicy').mockResolvedValue(makePolicy());
  vi.spyOn(backend.settings, 'listBackups').mockResolvedValue([]);
  vi.spyOn(backend.settings, 'listAuditEvents').mockResolvedValue([]);
  render(
    <MemoryRouter>
      <SettingsPage />
    </MemoryRouter>,
  );
  await screen.findByRole('heading', { name: '系统设置' });
  return { listMock };
}

/** 「已配置模型」表里包含某段文本的那一行。 */
function modelRow(text: string): HTMLElement {
  const table = screen.getByRole('table', { name: '已配置模型' });
  const cell = within(table).getByText(text);
  return cell.closest('tr') as HTMLElement;
}

/** `.env` 回落行（ai_model_configs 为空时后端从 settings 构造）。 */
function makeFallbackModel(overrides: Partial<AiModel> = {}): AiModel {
  return makeModel({
    model_id: 'gpt-4o',
    display_name: 'GPT-4o（.env 回落）',
    source: 'settings',
    api_key_configured: false,
    ...overrides,
  });
}

describe('SettingsPage — AI 自配置（AC-F1 Provider/端点/模型可编辑）', () => {
  it('AC-F1 编辑 .env 回落配置：保存走 POST，页面显示后端返回的生效值', async () => {
    const fallback = makeFallbackModel();
    const createdFromBackend = makeModel({
      model_id: 'glm-4-flash',
      display_name: 'GLM-4-Flash（免费）',
      base_url: 'https://open.bigmodel.cn/api/paas/v4',
      quality_tier: 'economy',
      source: 'database',
      is_default: true,
    });
    const { listMock } = await renderAiPage([
      { models: [fallback], routing: {} },
      { models: [createdFromBackend], routing: {} },
    ]);
    const createMock = vi.spyOn(backend.ai, 'createModel').mockResolvedValue(createdFromBackend);

    // 生效来源：.env 回落
    expect(await screen.findByText('.env 回落')).toBeInTheDocument();
    expect(
      screen.getByText(/数据库里还没有模型配置，当前生效值来自 \.env 回落/),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '编辑生效配置' }));
    // 表单已回填当前生效值（可编辑，不是只读 <td>）
    expect(screen.getByLabelText(/模型 ID/)).toHaveValue('gpt-4o');
    expect(screen.getByLabelText(/端点（Base URL）/)).toHaveValue('https://api.openai.com/v1');
    expect(screen.getByLabelText(/Provider/)).toHaveValue('openai_compatible');
    // .env 回落行不能直接 PUT，表单必须说明保存会写成数据库配置
    expect(
      screen.getByText(/该配置来自 \.env 回落，还不能直接修改：保存后会写入数据库/),
    ).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText(/模型 ID/), {
      target: { value: 'glm-4-flash' },
    });
    fireEvent.change(screen.getByLabelText(/端点（Base URL）/), {
      target: { value: 'https://open.bigmodel.cn/api/paas/v4' },
    });
    fireEvent.change(screen.getByLabelText(/显示名/), { target: { value: '本地草稿名' } });
    fireEvent.change(screen.getByLabelText(/层级/), { target: { value: 'economy' } });
    fireEvent.click(screen.getByRole('button', { name: '保存配置' }));

    await waitFor(() => expect(createMock).toHaveBeenCalledTimes(1));
    expect(createMock).toHaveBeenCalledWith({
      model_id: 'glm-4-flash',
      display_name: '本地草稿名',
      provider_type: 'openai_compatible',
      base_url: 'https://open.bigmodel.cn/api/paas/v4',
      quality_tier: 'economy',
      timeout: 60,
      temperature: 0.4,
      max_output_tokens: 1500,
      enabled: true,
      is_default: true,
    });
    // 保存后重新拉取列表，并显示后端返回的生效值（而不是草稿里的值）
    await waitFor(() => expect(listMock.mock.calls.length).toBeGreaterThanOrEqual(2));
    expect(
      await screen.findByText(/草稿生成使用数据库里的默认模型「glm-4-flash」/),
    ).toBeInTheDocument();
    expect(
      within(screen.getByRole('table', { name: '已配置模型' })).getByText(
        'GLM-4-Flash（免费）',
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText('本地草稿名')).not.toBeInTheDocument();
    expect(await screen.findByText(/模型配置「glm-4-flash」已创建/)).toBeInTheDocument();
  });

  it('AC-F1 编辑已有模型：走 PUT 局部更新，且不把 model_id 当可改字段提交', async () => {
    const row = makeModel({
      model_id: 'glm-4-flash',
      display_name: 'GLM-4',
      base_url: 'https://open.bigmodel.cn/api/paas/v4',
    });
    await renderAiPage([{ models: [row], routing: {} }]);
    const updateMock = vi
      .spyOn(backend.ai, 'updateModel')
      .mockResolvedValue({ ...row, timeout: 120 });

    fireEvent.click(screen.getByRole('button', { name: '编辑生效配置' }));
    expect(screen.getByLabelText(/模型 ID/)).toHaveValue('glm-4-flash');
    fireEvent.change(screen.getByLabelText(/超时（秒，1–600）/), {
      target: { value: '120' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存配置' }));

    await waitFor(() =>
      expect(updateMock).toHaveBeenCalledWith(
        'glm-4-flash',
        expect.objectContaining({ timeout: 120 }),
      ),
    );
    expect(updateMock.mock.calls[0][1]).not.toHaveProperty('model_id');
    expect(await screen.findByText(/模型配置「glm-4-flash」已更新/)).toBeInTheDocument();
  });
});

describe('SettingsPage — AI 自配置（AC-F2 API Key 走凭据库）', () => {
  it('AC-F2 选中模型后密钥写入该模型独立槽位，界面只显示「已配置」', async () => {
    const before = {
      models: [makeFallbackModel(), makeModel({ model_id: 'glm-4-flash', display_name: 'GLM-4', api_key_configured: false })],
      routing: {},
    };
    const after = {
      models: [makeFallbackModel(), makeModel({ model_id: 'glm-4-flash', display_name: 'GLM-4', api_key_configured: true })],
      routing: {},
    };
    await renderAiPage([before, after]);
    const setMock = vi.spyOn(backend.settings, 'setAiKey').mockResolvedValue({
      ai_enabled: false,
      key_masked: '…9f3a',
      model_id: 'glm-4-flash',
    });

    fireEvent.change(await screen.findByLabelText('作用范围'), {
      target: { value: 'glm-4-flash' },
    });
    const input = screen.getByLabelText('API Key');
    fireEvent.change(input, { target: { value: 'sk-test-model-slot-9f3a' } });
    fireEvent.click(screen.getByRole('button', { name: '保存密钥' }));

    await waitFor(() =>
      expect(setMock).toHaveBeenCalledWith('sk-test-model-slot-9f3a', 'glm-4-flash'),
    );
    expect(await screen.findByText(/尾号 …9f3a/)).toBeInTheDocument();
    // 保存后列表刷新 → 该模型 Key 变成「已配置」，但完整密钥不出现在界面上
    expect(
      await screen.findByText(/当前密钥状态：已配置（接口与界面均不回显完整密钥）/),
    ).toBeInTheDocument();
    expect(screen.queryByText(/sk-test-model-slot-9f3a/)).not.toBeInTheDocument();
    expect((input as HTMLInputElement).value).toBe('');
  });
});

describe('SettingsPage — AI 自配置（AC-F3 模型管理）', () => {
  it('AC-F3 设为默认调用后端并刷新列表', async () => {
    const a = makeModel({ model_id: 'model-a', display_name: 'Alpha', sort_order: 0 });
    const b = makeModel({
      model_id: 'model-b',
      display_name: 'Beta',
      quality_tier: 'premium',
      is_default: false,
      sort_order: 1,
    });
    const { listMock } = await renderAiPage([
      { models: [a, b], routing: {} },
      { models: [{ ...a, is_default: false }, { ...b, is_default: true }], routing: {} },
    ]);
    const defaultMock = vi
      .spyOn(backend.ai, 'setDefaultModel')
      .mockResolvedValue({ ...b, is_default: true });

    const row = await waitFor(() => modelRow('model-b'));
    fireEvent.click(within(row).getByRole('button', { name: '设为默认' }));

    await waitFor(() => expect(defaultMock).toHaveBeenCalledWith('model-b'));
    await waitFor(() => expect(listMock.mock.calls.length).toBeGreaterThanOrEqual(2));
    await waitFor(() => {
      expect(within(modelRow('model-b')).getByText('默认')).toBeInTheDocument();
      expect(within(modelRow('model-a')).queryByText('默认')).not.toBeInTheDocument();
    });
    expect(await screen.findByText(/已把「model-b」设为默认生效模型/)).toBeInTheDocument();
  });

  it('AC-F3 「编辑」把该行装进表单，保存走 PUT', async () => {
    const row = makeModel({
      model_id: 'nvidia/nemotron-3.5-super-vl:free',
      display_name: 'Nemotron',
      base_url: 'https://openrouter.ai/api/v1',
    });
    await renderAiPage([{ models: [row], routing: {} }]);
    const updateMock = vi
      .spyOn(backend.ai, 'updateModel')
      .mockResolvedValue({ ...row, max_output_tokens: 3000 });

    const tableRow = await waitFor(() => modelRow('nvidia/nemotron-3.5-super-vl:free'));
    fireEvent.click(within(tableRow).getByRole('button', { name: '编辑' }));
    expect(screen.getByLabelText(/模型 ID/)).toHaveValue('nvidia/nemotron-3.5-super-vl:free');
    expect(screen.getByLabelText(/端点（Base URL）/)).toHaveValue(
      'https://openrouter.ai/api/v1',
    );

    fireEvent.change(screen.getByLabelText(/最大输出 tokens（1–200000）/), {
      target: { value: '3000' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存配置' }));

    await waitFor(() =>
      expect(updateMock).toHaveBeenCalledWith(
        'nvidia/nemotron-3.5-super-vl:free',
        expect.objectContaining({ max_output_tokens: 3000 }),
      ),
    );
  });

  it('AC-F3 删除默认模型要确认（说明不会自动补默认），确认后列表刷新', async () => {
    const a = makeModel({ model_id: 'model-a', display_name: 'Alpha', sort_order: 0 });
    const b = makeModel({ model_id: 'model-b', display_name: 'Beta', is_default: false, sort_order: 1 });
    const { listMock } = await renderAiPage([
      { models: [a, b], routing: {} },
      { models: [b], routing: {} },
    ]);
    const deleteMock = vi
      .spyOn(backend.ai, 'deleteModel')
      .mockResolvedValue({ model_id: 'model-a', deleted: true });

    const row = await waitFor(() => modelRow('model-a'));
    fireEvent.click(within(row).getByRole('button', { name: '删除' }));

    const bar = screen.getByText((_, node) => !!node && node.classList.contains('confirm-bar'));
    expect(bar).toHaveTextContent('是当前默认模型');
    expect(bar).toHaveTextContent('不会自动指定新的默认模型');
    fireEvent.click(within(bar).getByRole('button', { name: '确认删除' }));

    await waitFor(() => expect(deleteMock).toHaveBeenCalledWith('model-a'));
    await waitFor(() =>
      expect(
        within(screen.getByRole('table', { name: '已配置模型' })).queryByText('model-a'),
      ).not.toBeInTheDocument(),
    );
    await waitFor(() => expect(listMock.mock.calls.length).toBeGreaterThanOrEqual(2));
  });

  it('AC-F3 .env 回落行不能删除（不是数据库记录）', async () => {
    await renderAiPage([{ models: [makeFallbackModel()], routing: {} }]);
    const row = await waitFor(() => modelRow('gpt-4o'));
    expect(within(row).getByRole('button', { name: '删除' })).toBeDisabled();
    expect(within(row).getByRole('button', { name: '设为默认' })).toBeDisabled();
  });
});

describe('SettingsPage — AI 自配置（AC-F4 校验与可读错误）', () => {
  async function openCreateForm() {
    await renderAiPage([{ models: [], routing: {} }]);
    fireEvent.click(await screen.findByRole('button', { name: '新增模型配置' }));
  }

  it('AC-F4 空模型 ID 被拦下，不发请求', async () => {
    await openCreateForm();
    const createMock = vi.spyOn(backend.ai, 'createModel');
    fireEvent.change(screen.getByLabelText(/端点（Base URL）/), {
      target: { value: 'https://api.example.com/v1' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存配置' }));
    expect(await screen.findByText('模型 ID 不能为空。')).toBeInTheDocument();
    expect(createMock).not.toHaveBeenCalled();
  });

  it('AC-F4 非 http(s) 端点被拦下，提示可读', async () => {
    await openCreateForm();
    const createMock = vi.spyOn(backend.ai, 'createModel');
    fireEvent.change(screen.getByLabelText(/模型 ID/), { target: { value: 'glm-4-flash' } });
    fireEvent.change(screen.getByLabelText(/端点（Base URL）/), {
      target: { value: 'ftp://example.com/v1' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存配置' }));
    expect(
      await screen.findByText('端点必须以 http:// 或 https:// 开头。'),
    ).toBeInTheDocument();
    expect(createMock).not.toHaveBeenCalled();
  });

  it('AC-F4 超时 / 温度 / 最大输出 tokens 的上下界被拦下', async () => {
    await openCreateForm();
    const createMock = vi.spyOn(backend.ai, 'createModel');
    fireEvent.change(screen.getByLabelText(/模型 ID/), { target: { value: 'glm-4-flash' } });
    fireEvent.change(screen.getByLabelText(/端点（Base URL）/), {
      target: { value: 'https://api.example.com/v1' },
    });

    fireEvent.change(screen.getByLabelText(/超时（秒，1–600）/), { target: { value: '900' } });
    fireEvent.click(screen.getByRole('button', { name: '保存配置' }));
    expect(await screen.findByText('超时（秒） 必须在 1–600 之间。')).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText(/超时（秒，1–600）/), { target: { value: '60' } });
    fireEvent.change(screen.getByLabelText(/温度（0–2）/), { target: { value: '2.5' } });
    fireEvent.click(screen.getByRole('button', { name: '保存配置' }));
    expect(await screen.findByText('采样温度 必须在 0–2 之间。')).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText(/温度（0–2）/), { target: { value: '0.4' } });
    fireEvent.change(screen.getByLabelText(/最大输出 tokens（1–200000）/), {
      target: { value: '0' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存配置' }));
    expect(
      await screen.findByText('最大输出 tokens 必须在 1–200000 之间。'),
    ).toBeInTheDocument();

    expect(createMock).not.toHaveBeenCalled();
  });

  it('AC-F4 后端 409 显示可读提示（而不是原始 4xx 报文）', async () => {
    await renderAiPage([{ models: [], routing: {} }]);
    const createMock = vi
      .spyOn(backend.ai, 'createModel')
      .mockRejectedValue(
        new ApiError(
          409,
          {
            error: {
              code: 'ai_model_exists',
              message: "Model configuration 'glm-4-flash' already exists.",
              details: { model_id: 'glm-4-flash' },
            },
          },
          'fallback',
        ),
      );

    fireEvent.click(await screen.findByRole('button', { name: '新增模型配置' }));
    fireEvent.change(screen.getByLabelText(/模型 ID/), { target: { value: 'glm-4-flash' } });
    fireEvent.change(screen.getByLabelText(/端点（Base URL）/), {
      target: { value: 'https://api.example.com/v1' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存配置' }));

    await waitFor(() => expect(createMock).toHaveBeenCalled());
    expect(
      await screen.findByText(/该模型 ID 已存在：请直接编辑已配置的模型，或换一个 ID。/),
    ).toBeInTheDocument();
  });
});

describe('SettingsPage — AI 自配置（生效来源提示）', () => {
  it('生效模型来自数据库默认模型时，说明改 .env 不会改变生效值', async () => {
    await renderAiPage([
      { models: [makeModel({ model_id: 'glm-4-flash', display_name: 'GLM-4' })], routing: {} },
    ]);
    expect(await screen.findByText('数据库配置')).toBeInTheDocument();
    expect(
      screen.getByText(/草稿生成使用数据库里的默认模型「glm-4-flash」/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        /修改 backend\/\.env 的 AI_MODEL \/ AI_BASE_URL 不会改变这里的生效值/,
      ),
    ).toBeInTheDocument();
  });
});
