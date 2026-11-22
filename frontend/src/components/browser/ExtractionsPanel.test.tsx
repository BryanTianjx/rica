import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { browserApi } from '../../services/endpoints';
import { ExtractionsPanel } from './ExtractionsPanel';
import type { BrowserExtraction } from '../../types';

function makeExtraction(overrides: Partial<BrowserExtraction> = {}): BrowserExtraction {
  return {
    id: 'ext_1',
    status: 'draft',
    fields: { name: '张三', job_intention: '前端工程师', expected_salary: null },
    field_origins: {},
    missing_fields: [],
    position_hint: null,
    company_hint: null,
    mask_hits: 0,
    provenance: {
      session_id: 'ses_abc',
      snapshot_action_id: 'act_9',
      source_url: 'http://127.0.0.1:18799/fixture.html',
      source_domain: '127.0.0.1',
      snapshot_sha256: 'aa11',
      snapshot_path: 'C:\\snap.json',
      snapshot_node_count: 12,
      snapshot_masked_fields: 0,
    },
    duplicate: null,
    review_digest: null,
    reviewed_at: null,
    review_note: null,
    import_session_id: 'imp_1',
    candidate_id: null,
    confirmed_at: null,
    revisions: [],
    created_at: '2026-11-18T15:00:00Z',
    updated_at: '2026-11-18T15:00:00Z',
    ...overrides,
  };
}

describe('ExtractionsPanel', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('edits a field, shows the before value, and saves the patch', async () => {
    const patchSpy = vi
      .spyOn(browserApi, 'patchExtraction')
      .mockResolvedValue({ extraction: makeExtraction({ fields: { name: '张三丰', job_intention: '前端工程师' } }) });
    vi.spyOn(browserApi, 'listExtractions')
      .mockResolvedValueOnce({ extractions: [makeExtraction()] })
      .mockResolvedValue({
        extractions: [
          makeExtraction({
            fields: { name: '张三丰', job_intention: '前端工程师' },
            revisions: [
              { revision_no: 1, changes: [{ field: 'name', before: '张三', after: '张三丰' }], note: null, created_at: '2026-11-18T15:10:00Z' },
            ],
          }),
        ],
      });
    vi.spyOn(browserApi, 'getExtraction').mockResolvedValue({
      extraction: makeExtraction({
        fields: { name: '张三丰', job_intention: '前端工程师' },
        revisions: [
          { revision_no: 1, changes: [{ field: 'name', before: '张三', after: '张三丰' }], note: null, created_at: '2026-11-18T15:10:00Z' },
        ],
      }),
    });

    render(<ExtractionsPanel />);
    await waitFor(() => expect(screen.getByText('张三')).toBeTruthy());
    fireEvent.click(screen.getByText('张三'));
    fireEvent.change(screen.getByLabelText('修订 姓名'), { target: { value: '张三丰' } });
    // 保存前就能看到 before（改前：张三）
    expect(screen.getByText(/改前：张三/)).toBeTruthy();
    fireEvent.click(screen.getByText(/保存修订（1）/));
    await waitFor(() => expect(patchSpy).toHaveBeenCalledWith('ext_1', { name: '张三丰' }));
    await waitFor(() => expect(screen.getByText('修订历史')).toBeTruthy());
    expect(screen.getByText(/张三 → 张三丰/)).toBeTruthy();
  });

  it('approve then confirm writes to a real candidate with a field summary', async () => {
    const approveSpy = vi
      .spyOn(browserApi, 'reviewExtraction')
      .mockResolvedValue({ extraction: makeExtraction({ status: 'approved' }) });
    vi.spyOn(browserApi, 'listExtractions')
      .mockResolvedValueOnce({ extractions: [makeExtraction()] })
      .mockResolvedValue({ extractions: [makeExtraction({ status: 'approved' })] });
    const confirmSpy = vi
      .spyOn(browserApi, 'confirmExtraction')
      .mockResolvedValue({
        session_id: 'imp_1',
        extraction_id: 'ext_1',
        candidate_id: 'cand_777',
        duplicate_kind: null,
        duplicate_override: false,
        fields_written: ['name', 'job_intention'],
      });

    render(<ExtractionsPanel />);
    await waitFor(() => expect(screen.getByText('张三')).toBeTruthy());
    fireEvent.click(screen.getByText('张三'));
    fireEvent.click(screen.getByText('审批通过'));
    await waitFor(() => expect(approveSpy).toHaveBeenCalledWith('ext_1', 'approve'));

    // reload 完成后按钮才从「先审批通过才能落档」的禁用态释放
    const confirmBtn = await waitFor(
      () => {
        const btn = screen.getByText('落档到候选人') as HTMLButtonElement;
        expect(btn.disabled).toBe(false);
        return btn;
      },
    );
    fireEvent.click(confirmBtn);
    // 摘要必须先出现：字段 + 来源，因为这一步写正式业务数据
    expect(screen.getByText(/写入字段/)).toBeTruthy();
    expect(screen.getByText(/姓名=张三/)).toBeTruthy();
    expect((await screen.findAllByText(/127\.0\.0\.1:18799\/fixture\.html/)).length).toBeGreaterThan(1);
    fireEvent.click(screen.getByText('确认落档'));
    await waitFor(() => expect(confirmSpy).toHaveBeenCalledWith('imp_1', { confirm_duplicate: false }));
    expect(screen.getByText(/已落档为候选人 cand_777/)).toBeTruthy();
  });

  it('blocks confirming past a duplicate verdict until the checkbox is ticked', async () => {
    vi.spyOn(browserApi, 'listExtractions').mockResolvedValue({
      extractions: [makeExtraction({ status: 'approved', duplicate: { kind: 'same_name_phone', candidate_id: 'cand_1', evidence: {} } })],
    });
    render(<ExtractionsPanel />);
    await waitFor(() => expect(screen.getByText('张三')).toBeTruthy());
    fireEvent.click(screen.getByText('张三'));
    const confirmBtn = screen.getByText('落档到候选人') as HTMLButtonElement;
    expect(confirmBtn.disabled).toBe(true);
    expect((await screen.findAllByText(/重复判定：same_name_phone/)).length).toBeGreaterThan(0);
    fireEvent.click(screen.getByLabelText('确认越过重复判定'));
    expect((screen.getByText('落档到候选人') as HTMLButtonElement).disabled).toBe(false);
  });
});
