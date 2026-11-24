/**
 * Phase 4 — AI 工作空间（基于真实后端契约，不做通用聊天空壳）。
 *
 * 真实能力只有：候选人上下文生成草稿（/drafts/generate）、草稿审核、
 * 模型注册与场景路由、AI 出网披露。本页把这些能力聚合成一个入口，
 * 并明确「AI 只起草，人工审核，Approved 不代表已发送」的边界。
 */
import { Link } from 'react-router-dom';
import { PageHeader } from '../app/layout/PageHeader';
import { useAsync } from '../hooks/useAsync';
import { backend } from '../services/backend';
import { Badge, EmptyState, ErrorBox, Loading } from '../components/ui';
import { DRAFT_STATUS } from '../shared/statusVocabulary';
import type { DraftStatus } from '../types';

const DRAFT_FLOW: { status: DraftStatus; label: string }[] = [
  { status: DRAFT_STATUS.Generated, label: '已生成 · 待审核' },
  { status: DRAFT_STATUS.Edited, label: '人工编辑中' },
  { status: DRAFT_STATUS.Approved, label: '已批准（未发送）' },
  { status: DRAFT_STATUS.Copied, label: '已复制（发送由人完成）' },
  { status: DRAFT_STATUS.Rejected, label: '已拒绝' },
];

export function AiWorkspacePage() {
  const models = useAsync(() => backend.ai.listModels(), []);
  const routing = useAsync(() => backend.ai.getRouting(), []);
  const egress = useAsync(() => backend.settings.getAiEgress(), []);
  const drafts = useAsync(() => backend.drafts.list(), []);

  const pending = drafts.data?.items.filter((draft) => draft.status === DRAFT_STATUS.Generated) ?? [];
  const sources = [
    { name: '模型列表', error: models.error, retry: models.reload },
    { name: '场景路由', error: routing.error, retry: routing.reload },
    { name: 'AI 出网披露', error: egress.error, retry: egress.reload },
    { name: '草稿列表', error: drafts.error, retry: drafts.reload },
  ];
  const failed = sources.filter((source) => source.error);

  return (
    <>
      <PageHeader
        title="AI 工作空间"
        description={<p>AI 只起草与辅助分析；发送与业务写入始终由你手动完成。</p>}
        actions={
          <Link className="btn primary" to="/drafts">
            打开草稿审核
          </Link>
        }
      />
      <ErrorBoxList errors={failed.map((f) => `${f.name}：${f.error}`)} />

      <div className="grid cols-2">
        <section className="card">
          <div className="card-head"><h2>AI 服务状态</h2></div>
          {models.loading && <Loading />}
          {models.data && (
            <>
              <p className="muted small">
                已配置模型 {models.data.models.length} 个；默认模型：
                {models.data.models.find((m) => m.is_default)?.display_name ?? '未设置'}
              </p>
              <ul>
                {models.data.models.slice(0, 6).map((model) => (
                  <li key={model.model_id}>
                    {model.display_name || model.model_id}{' '}
                    <Badge tone={model.enabled ? 'ok' : 'default'}>{model.enabled ? '启用' : '停用'}</Badge>{' '}
                    <span className="muted small">{model.source}</span>
                  </li>
                ))}
              </ul>
              {models.data.models.length === 0 && <EmptyState message="尚未配置模型；到系统设置 → 模型管理添加。" />}
            </>
          )}
        </section>

        <section className="card">
          <div className="card-head"><h2>出网与隐私边界</h2></div>
          {egress.loading && <Loading />}
          {egress.data && (
            <>
              <p>{egress.data.notice}</p>
              <p className="muted small">
                Provider：{egress.data.provider} · 模型：{egress.data.model} · 出网：
                <Badge tone={egress.data.enabled && egress.data.consent_granted ? 'warn' : 'ok'}>
                  {egress.data.enabled && egress.data.consent_granted ? '已授权' : '未授权/关闭'}
                </Badge>
              </p>
              <p className="muted small">脱敏字段：{egress.data.masked_fields.join('、') || '无'}</p>
            </>
          )}
        </section>
      </div>

      <section className="card">
        <div className="card-head"><h2>待审核草稿</h2></div>
        {drafts.loading && <Loading />}
        {drafts.data && pending.length === 0 && <EmptyState message="没有待审核草稿。到候选人详情生成。" />}
        {pending.slice(0, 8).map((draft) => (
          <div className="row" key={draft.id}>
            <span>{draft.message_text.slice(0, 60)}…</span>
            <Badge tone="warn">待审核</Badge>
            <Link className="small" to="/drafts">去审核</Link>
          </div>
        ))}
      </section>

      <section className="card">
        <div className="card-head"><h2>生成到发送的状态链</h2></div>
        <ol>
          {DRAFT_FLOW.map((step) => (
            <li key={step.status}>{step.label}</li>
          ))}
        </ol>
        <p className="muted small">Approved 只表示人工批准，不代表消息已经发送。</p>
      </section>

      {routing.data && (
        <section className="card">
          <div className="card-head"><h2>场景路由概览</h2></div>
          <p className="muted small">共 {Object.keys(routing.data.routing).length} 个场景；编辑请到系统设置 → 场景路由。</p>
        </section>
      )}
    </>
  );
}

function ErrorBoxList({ errors }: { errors: string[] }) {
  if (errors.length === 0) return null;
  return <ErrorBox message={`部分数据不可用：${errors.join('；')}`} />;
}
