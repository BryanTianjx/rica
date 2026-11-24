import { useState } from 'react';
import { PageHeader } from '../app/layout/PageHeader';
import { Link } from 'react-router-dom';
import { useAsync } from '../hooks/useAsync';
import { backend, describeApiError } from '../services/backend';
import { ConfirmBar } from '../components/ConfirmBar';
import { PartialNotice } from '../shared/components/PartialNotice';
import { useToast } from '../shared/components/ToastViewport';
import { EmptyState, ErrorBox, Loading, StatusBadge } from '../components/ui';
import type { Candidate, CandidateStatus } from '../types';

const STATUSES = [
  'New',
  'Contacted',
  'ReadNoReply',
  'Replied',
  'Active',
  'Interview',
  'Rejected',
  'Withdrawn',
  'Closed',
];

type EditingRow = {
  id: string;
  name: string;
  position_id: string;
  status: CandidateStatus;
  expected_salary: string;
  expected_location: string;
  job_intention: string;
  notes: string;
};

export function CandidatesPage() {
  const [search, setSearch] = useState('');
  const [status, setStatus] = useState('');
  const [showForm, setShowForm] = useState(false);
  const [name, setName] = useState('');
  const [positionId, setPositionId] = useState('');
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  const [editing, setEditing] = useState<EditingRow | null>(null);
  const [savingId, setSavingId] = useState<string | null>(null);
  const [rowError, setRowError] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const toast = useToast();

  const candidates = useAsync(
    () => backend.candidates.list({ search, status: status || undefined }),
    [search, status],
  );
  const positions = useAsync(() => backend.positions.list({ limit: 500 }), []);

  async function createCandidate(event: React.FormEvent) {
    event.preventDefault();
    setSaving(true);
    setFormError(null);
    setSaved(false);
    try {
      await backend.candidates.create({
        name,
        position_id: positionId || undefined,
      });
      setName('');
      setPositionId('');
      setShowForm(false);
      setSaved(true);
      toast.push('候选人已创建', 'ok');
      candidates.reload();
    } catch (caught: unknown) {
      setFormError(describeApiError(caught, '创建失败'));
    } finally {
      setSaving(false);
    }
  }

  function startEdit(candidate: Candidate) {
    setEditing({
      id: candidate.id,
      name: candidate.name,
      position_id: candidate.position_id ?? '',
      status: candidate.status,
      expected_salary: candidate.expected_salary ?? '',
      expected_location: candidate.expected_location ?? '',
      job_intention: candidate.job_intention ?? '',
      notes: candidate.notes ?? '',
    });
    setRowError(null);
  }

  function cancelEdit() {
    setEditing(null);
    setRowError(null);
  }

  async function saveEdit() {
    if (!editing) return;
    setSavingId(editing.id);
    setRowError(null);
    try {
      await backend.candidates.update(editing.id, {
        name: editing.name || undefined,
        // An explicitly emptied selector means "no position": send null so the
        // PATCH really unbinds. `undefined` would drop the key and leave the
        // existing link untouched (backend keeps it when the field is absent).
        position_id: editing.position_id || null,
        status: editing.status,
        expected_salary: editing.expected_salary || undefined,
        expected_location: editing.expected_location || undefined,
        job_intention: editing.job_intention || undefined,
        notes: editing.notes || undefined,
      });
      setEditing(null);
      candidates.reload();
    } catch (caught: unknown) {
      setRowError(describeApiError(caught, '保存失败'));
    } finally {
      setSavingId(null);
    }
  }

  async function toggleArchive(candidate: Candidate) {
    setSavingId(candidate.id);
    setRowError(null);
    try {
      await backend.candidates.update(candidate.id, {
        archived: !candidate.archived,
      });
      candidates.reload();
    } catch (caught: unknown) {
      setRowError(describeApiError(caught, '操作失败'));
    } finally {
      setSavingId(null);
    }
  }

  async function confirmDelete(candidate: Candidate) {
    setSavingId(candidate.id);
    setRowError(null);
    try {
      await backend.candidates.remove(candidate.id);
      setDeletingId(null);
      candidates.reload();
    } catch (caught: unknown) {
      setRowError(describeApiError(caught, '删除失败'));
      setDeletingId(null);
    } finally {
      setSavingId(null);
    }
  }

  return (
    <>
      <PageHeader
        title="候选人"
        description={<p>管理候选人档案、岗位关联与状态。状态只能由你手动变更。</p>}
        actions={
          <button onClick={() => setShowForm((value) => !value)}>
            {showForm ? '取消' : '新增候选人'}
          </button>
        }
      />

      {saved && (
        <div className="notice" aria-live="polite">
          候选人已创建。
        </div>
      )}

      <PartialNotice
        sources={[{ name: '岗位列表', error: positions.error, retry: positions.reload }]}
      />

      {rowError && <ErrorBox message={rowError} />}

      {showForm && (
        <div className="card">
          <h2>新增候选人</h2>
          {formError && <ErrorBox message={formError} />}
          <form onSubmit={createCandidate}>
            <div className="form-row two">
              <div>
                <label htmlFor="cand-name">姓名 / 显示名称 *</label>
                <input
                  id="cand-name"
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                  required
                />
              </div>
              <div>
                <label htmlFor="cand-position">关联岗位</label>
                <select
                  id="cand-position"
                  value={positionId}
                  onChange={(event) => setPositionId(event.target.value)}
                >
                  <option value="">（暂不关联）</option>
                  {positions.data?.items.map((position) => (
                    <option key={position.id} value={position.id}>
                      {position.name}
                    </option>
                  ))}
                </select>
              </div>
            </div>
            <div className="actions">
              <button className="primary" type="submit" disabled={saving || !name.trim()}>
                {saving ? '保存中…' : '保存'}
              </button>
            </div>
          </form>
        </div>
      )}

      <div className="toolbar">
        <input
          placeholder="搜索姓名、技能、地点…"
          value={search}
          onChange={(event) => setSearch(event.target.value)}
        />
        <select value={status} onChange={(event) => setStatus(event.target.value)}>
          <option value="">全部状态</option>
          {STATUSES.map((value) => (
            <option key={value} value={value}>
              {value}
            </option>
          ))}
        </select>
      </div>

      {candidates.loading && <Loading />}
      {candidates.error && <ErrorBox message={candidates.error} />}
      {candidates.data && candidates.data.items.length === 0 && (
        <EmptyState message="没有匹配的候选人" />
      )}
      {candidates.data && candidates.data.items.length > 0 && (
        <div className="card">
          <table>
            <thead>
              <tr>
                <th>姓名</th>
                <th>岗位</th>
                <th>状态</th>
                <th>消息</th>
                <th>跟进中</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {candidates.data.items.map((candidate: Candidate) =>
                editing?.id === candidate.id ? (
                  <tr key={candidate.id}>
                    <td>
                      <input
                        value={editing.name}
                        onChange={(event) =>
                          setEditing({ ...editing, name: event.target.value })
                        }
                      />
                    </td>
                    <td>
                      <select
                        aria-label="关联岗位"
                        value={editing.position_id}
                        onChange={(event) =>
                          setEditing({ ...editing, position_id: event.target.value })
                        }
                      >
                        <option value="">（暂不关联）</option>
                        {positions.data?.items.map((position) => (
                          <option key={position.id} value={position.id}>
                            {position.name}
                          </option>
                        ))}
                      </select>
                    </td>
                    <td>
                      <select
                        value={editing.status}
                        onChange={(event) =>
                          setEditing({ ...editing, status: event.target.value as CandidateStatus })
                        }
                      >
                        {STATUSES.map((value) => (
                          <option key={value} value={value}>
                            {value}
                          </option>
                        ))}
                      </select>
                    </td>
                    <td>{candidate.message_count}</td>
                    <td>{candidate.active_follow_up_count}</td>
                    <td>
                      <div className="actions">
                        <button
                          className="primary"
                          onClick={saveEdit}
                          disabled={savingId === candidate.id || !editing.name.trim()}
                        >
                          {savingId === candidate.id ? '保存中…' : '保存'}
                        </button>
                        <button onClick={cancelEdit} disabled={savingId === candidate.id}>
                          取消
                        </button>
                      </div>
                    </td>
                  </tr>
                ) : (
                  <tr key={candidate.id}>
                    <td>
                      <Link to={`/candidates/${candidate.id}`}>{candidate.name}</Link>
                    </td>
                    <td className="muted">{candidate.position_name ?? '—'}</td>
                    <td>
                      <StatusBadge status={candidate.status} />
                    </td>
                    <td>{candidate.message_count}</td>
                    <td>{candidate.active_follow_up_count}</td>
                    <td>
                      <div className="actions">
                        <button onClick={() => startEdit(candidate)}>编辑</button>
                        <button
                          onClick={() => toggleArchive(candidate)}
                          disabled={savingId === candidate.id}
                        >
                          {candidate.archived ? '取消归档' : '归档'}
                        </button>
                        <button
                          className="danger"
                          onClick={() => setDeletingId(candidate.id)}
                          disabled={savingId === candidate.id}
                        >
                          删除
                        </button>
                      </div>
                      {deletingId === candidate.id && (
                        <ConfirmBar
                          message={<>确定删除「{candidate.name}」？此操作不可恢复。</>}
                          onConfirm={() => confirmDelete(candidate)}
                          onCancel={() => setDeletingId(null)}
                          busy={savingId === candidate.id}
                          busyLabel="删除中…"
                          confirmLabel="确认删除"
                        />
                      )}
                    </td>
                  </tr>
                ),
              )}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}
