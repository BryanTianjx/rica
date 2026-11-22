import { useState } from 'react';
import { useAsync } from '../hooks/useAsync';
import { positionsApi } from '../services/endpoints';
import { ConfirmBar } from '../components/ConfirmBar';
import { EmptyState, ErrorBox, Loading, StatusBadge } from '../components/ui';
import type { Position, PositionStatus } from '../types';
import { DEFAULT_POSITION_STATUS, POSITION_STATUSES } from '../positionStatuses';

/**
 * 编辑交互：表单式（一条记录装载进上方表单卡片），不保留 per-row 行内编辑。
 *
 * main 的行内编辑只覆盖最初 4 个字段；合并后同一页面要暴露 10 个字段
 * （岗位迁移带来的 9 个扩展字段）。把 10 个受控输入塞进一个 <tr> 会撑坏表格布局，
 * 薪资跨字段校验的报错也无处安放，还要把创建侧的表单/校验/载荷逻辑再抄一遍。
 * 因此创建与编辑共用一套表单与 `submitForm` 校验路径，行内「编辑」按钮只负责
 * 把该记录装进表单。状态词表仍走 `../positionStatuses`（C8：字面量不按页面重复）。
 */

/** Every editable position field, kept as a string so inputs stay controlled. */
type FormValues = {
  name: string;
  description: string;
  location: string;
  skills: string;
  experience: string;
  education: string;
  salaryMin: string;
  salaryMax: string;
  notes: string;
  status: PositionStatus;
};

const EMPTY_FORM: FormValues = {
  name: '',
  description: '',
  location: '',
  skills: '',
  experience: '',
  education: '',
  salaryMin: '',
  salaryMax: '',
  notes: '',
  status: DEFAULT_POSITION_STATUS,
};

function formFrom(position: Position): FormValues {
  return {
    name: position.name,
    description: position.description ?? '',
    location: position.location ?? '',
    skills: position.skill_requirements ?? '',
    experience: position.experience_requirements ?? '',
    education: position.education_requirements ?? '',
    salaryMin: position.salary_min == null ? '' : String(position.salary_min),
    salaryMax: position.salary_max == null ? '' : String(position.salary_max),
    notes: position.notes ?? '',
    status: position.status,
  };
}

/** Blank text means "no value"; on update it is sent as an explicit null. */
function textOrNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

/** Raw salary input -> number (NaN for garbage), null when left empty. */
function parseSalary(raw: string): number | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  return Number(trimmed);
}

function salaryIssue(values: FormValues): string | null {
  const min = parseSalary(values.salaryMin);
  const max = parseSalary(values.salaryMax);
  if (min !== null && Number.isNaN(min)) return '最低薪资必须是数字。';
  if (max !== null && Number.isNaN(max)) return '最高薪资必须是数字。';
  if (min !== null && min < 0) return '最低薪资不能为负数。';
  if (max !== null && max < 0) return '最高薪资不能为负数。';
  if (min !== null && max !== null && min > max) return '最低薪资不能高于最高薪资。';
  return null;
}

/**
 * Create payload omits empty optional fields so the server keeps its defaults
 * (and the request stays exactly as small as the data warrants).
 */
function buildCreatePayload(values: FormValues): Partial<Position> & { name: string } {
  const payload: Partial<Position> & { name: string } = { name: values.name.trim() };
  const description = textOrNull(values.description);
  if (description) payload.description = description;
  const location = textOrNull(values.location);
  if (location) payload.location = location;
  const skills = textOrNull(values.skills);
  if (skills) payload.skill_requirements = skills;
  const experience = textOrNull(values.experience);
  if (experience) payload.experience_requirements = experience;
  const education = textOrNull(values.education);
  if (education) payload.education_requirements = education;
  const notes = textOrNull(values.notes);
  if (notes) payload.notes = notes;
  const min = parseSalary(values.salaryMin);
  if (min !== null) payload.salary_min = min;
  const max = parseSalary(values.salaryMax);
  if (max !== null) payload.salary_max = max;
  // Only send a status the user actually changed away from the server default.
  if (values.status !== DEFAULT_POSITION_STATUS) payload.status = values.status;
  return payload;
}

/** Update payload always carries every field so clearing a value really clears it. */
function buildUpdatePayload(values: FormValues): Partial<Position> {
  return {
    name: values.name.trim(),
    description: textOrNull(values.description),
    location: textOrNull(values.location),
    skill_requirements: textOrNull(values.skills),
    experience_requirements: textOrNull(values.experience),
    education_requirements: textOrNull(values.education),
    salary_min: parseSalary(values.salaryMin),
    salary_max: parseSalary(values.salaryMax),
    notes: textOrNull(values.notes),
    status: values.status,
  };
}

function salaryLabel(position: Position): string {
  if (position.salary_min === null && position.salary_max === null) return '—';
  if (position.salary_min !== null && position.salary_max !== null) {
    return `${position.salary_min}–${position.salary_max}`;
  }
  return String(position.salary_min ?? position.salary_max);
}

export function PositionsPage() {
  const [search, setSearch] = useState('');
  const [includeArchived, setIncludeArchived] = useState(false);
  const positions = useAsync(
    () =>
      positionsApi.list({
        search: search || undefined,
        include_archived: includeArchived || undefined,
      }),
    [search, includeArchived],
  );

  const [draft, setDraft] = useState<FormValues | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const [savingId, setSavingId] = useState<string | null>(null);
  const [rowError, setRowError] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);

  function openCreateForm() {
    setDraft({ ...EMPTY_FORM });
    setEditingId(null);
    setError(null);
    setNotice(null);
  }

  function closeForm() {
    setDraft(null);
    setEditingId(null);
    setError(null);
  }

  function startEdit(position: Position) {
    setDraft(formFrom(position));
    setEditingId(position.id);
    setError(null);
    setNotice(null);
    setRowError(null);
  }

  function patchDraft(patch: Partial<FormValues>) {
    setDraft((current) => (current ? { ...current, ...patch } : current));
  }

  async function submitForm(event: React.FormEvent) {
    event.preventDefault();
    if (!draft) return;
    if (!draft.name.trim()) {
      setError('岗位名称不能为空。');
      return;
    }
    const issue = salaryIssue(draft);
    if (issue) {
      setError(issue);
      return;
    }
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      if (editingId) {
        await positionsApi.update(editingId, buildUpdatePayload(draft));
        setNotice('岗位已更新。');
      } else {
        await positionsApi.create(buildCreatePayload(draft));
        setNotice('岗位已创建。');
      }
      setDraft(null);
      setEditingId(null);
      positions.reload();
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : '保存失败');
    } finally {
      setSaving(false);
    }
  }

  async function toggleArchive(position: { id: string; archived: boolean }) {
    setSavingId(position.id);
    setRowError(null);
    try {
      await positionsApi.update(position.id, {
        archived: !position.archived,
      });
      positions.reload();
    } catch (caught: unknown) {
      setRowError(caught instanceof Error ? caught.message : '操作失败');
    } finally {
      setSavingId(null);
    }
  }

  async function confirmDelete(id: string) {
    setSavingId(id);
    setRowError(null);
    try {
      await positionsApi.remove(id);
      setDeletingId(null);
      positions.reload();
    } catch (caught: unknown) {
      setRowError(caught instanceof Error ? caught.message : '删除失败');
      setDeletingId(null);
    } finally {
      setSavingId(null);
    }
  }

  return (
    <>
      <div className="page-head">
        <div>
          <h1>岗位管理</h1>
          <p>岗位要求只作为匹配和沟通参考，不会被写成候选人的真实经历。</p>
        </div>
        <div className="actions">
          <button onClick={openCreateForm}>新增岗位</button>
        </div>
      </div>

      {notice && (
        <div className="notice" aria-live="polite">
          {notice}
        </div>
      )}
      {error && <ErrorBox message={error} />}
      {rowError && <ErrorBox message={rowError} />}

      {draft && (
        <div className="card">
          <h2>{editingId ? '编辑岗位' : '新增岗位'}</h2>
          <form onSubmit={submitForm}>
            <div className="form-row two">
              <div>
                <label htmlFor="pos-name">岗位名称 *</label>
                <input
                  id="pos-name"
                  value={draft.name}
                  onChange={(event) => patchDraft({ name: event.target.value })}
                  required
                />
              </div>
              <div>
                <label htmlFor="pos-location">工作地点</label>
                <input
                  id="pos-location"
                  value={draft.location}
                  onChange={(event) => patchDraft({ location: event.target.value })}
                />
              </div>
            </div>
            <div className="form-row two">
              <div>
                <label htmlFor="pos-status">岗位状态</label>
                <select
                  id="pos-status"
                  value={draft.status}
                  onChange={(event) =>
                    patchDraft({ status: event.target.value as PositionStatus })
                  }
                >
                  {POSITION_STATUSES.map((value) => (
                    <option key={value} value={value}>
                      {value}
                    </option>
                  ))}
                </select>
              </div>
              <div>
                <label htmlFor="pos-education">学历要求</label>
                <input
                  id="pos-education"
                  value={draft.education}
                  onChange={(event) => patchDraft({ education: event.target.value })}
                />
              </div>
            </div>
            <div className="form-row">
              <label htmlFor="pos-description">岗位描述</label>
              <textarea
                id="pos-description"
                value={draft.description}
                onChange={(event) => patchDraft({ description: event.target.value })}
                rows={3}
              />
            </div>
            <div className="form-row">
              <label htmlFor="pos-skills">技能要求</label>
              <textarea
                id="pos-skills"
                value={draft.skills}
                onChange={(event) => patchDraft({ skills: event.target.value })}
                rows={3}
              />
            </div>
            <div className="form-row">
              <label htmlFor="pos-experience">经验要求</label>
              <textarea
                id="pos-experience"
                value={draft.experience}
                onChange={(event) => patchDraft({ experience: event.target.value })}
                rows={2}
              />
            </div>
            <div className="form-row two">
              <div>
                <label htmlFor="pos-salary-min">最低薪资</label>
                <input
                  id="pos-salary-min"
                  type="number"
                  min={0}
                  step={1}
                  value={draft.salaryMin}
                  onChange={(event) => patchDraft({ salaryMin: event.target.value })}
                />
              </div>
              <div>
                <label htmlFor="pos-salary-max">最高薪资</label>
                <input
                  id="pos-salary-max"
                  type="number"
                  min={0}
                  step={1}
                  value={draft.salaryMax}
                  onChange={(event) => patchDraft({ salaryMax: event.target.value })}
                />
              </div>
            </div>
            <div className="form-row">
              <label htmlFor="pos-notes">备注</label>
              <textarea
                id="pos-notes"
                value={draft.notes}
                onChange={(event) => patchDraft({ notes: event.target.value })}
                rows={2}
              />
            </div>
            <div className="actions">
              <button className="primary" type="submit" disabled={saving || !draft.name.trim()}>
                {saving ? '保存中…' : '保存'}
              </button>
              <button type="button" onClick={closeForm} disabled={saving}>
                取消
              </button>
            </div>
          </form>
        </div>
      )}

      <div className="toolbar">
        <input
          placeholder="搜索岗位名称、地点、技能…"
          aria-label="搜索岗位"
          value={search}
          onChange={(event) => setSearch(event.target.value)}
        />
        <button
          type="button"
          aria-pressed={includeArchived}
          onClick={() => setIncludeArchived((value) => !value)}
        >
          {includeArchived ? '隐藏已归档' : '显示已归档'}
        </button>
      </div>

      {positions.loading && <Loading />}
      {positions.error && <ErrorBox message={positions.error} />}
      {positions.data && positions.data.items.length === 0 && (
        <EmptyState message="还没有岗位" />
      )}
      {positions.data && positions.data.items.length > 0 && (
        <div className="card">
          <table>
            <thead>
              <tr>
                <th>岗位</th>
                <th>地点</th>
                <th>技能要求</th>
                <th>薪资</th>
                <th>状态</th>
                <th>候选人</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {positions.data.items.map((position) => (
                <tr key={position.id}>
                  <td>
                    {position.name}
                    {position.archived && <span className="muted small"> (已归档)</span>}
                  </td>
                  <td className="muted">{position.location ?? '—'}</td>
                  <td className="muted small">{position.skill_requirements ?? '—'}</td>
                  <td className="muted small">{salaryLabel(position)}</td>
                  <td>
                    <StatusBadge status={position.status} />
                  </td>
                  <td>{position.candidate_count}</td>
                  <td>
                    <div className="actions">
                      <button onClick={() => startEdit(position)}>编辑</button>
                      <button
                        onClick={() => toggleArchive(position)}
                        disabled={savingId === position.id}
                      >
                        {position.archived ? '取消归档' : '归档'}
                      </button>
                      <button
                        className="danger"
                        onClick={() => setDeletingId(position.id)}
                        disabled={savingId === position.id}
                      >
                        删除
                      </button>
                    </div>
                    {deletingId === position.id && (
                      <ConfirmBar
                        message={<>确定删除「{position.name}」？此操作不可恢复。</>}
                        onConfirm={() => confirmDelete(position.id)}
                        onCancel={() => setDeletingId(null)}
                        busy={savingId === position.id}
                        busyLabel="删除中…"
                        confirmLabel="确认删除"
                      />
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}
