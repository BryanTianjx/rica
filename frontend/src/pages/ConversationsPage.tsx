import { useState } from 'react';
import { PageHeader } from '../app/layout/PageHeader';
import { Link } from 'react-router-dom';
import { useAsync } from '../hooks/useAsync';
import { backend } from '../services/backend';
import { EmptyState, ErrorBox, Loading, LocalTime } from '../components/ui';
import { PartialNotice } from '../shared/components/PartialNotice';
import type { Conversation } from '../types';

export function ConversationsPage() {
  const candidates = useAsync(() => backend.candidates.list(), []);
  const conversations = useAsync(() => backend.conversations.list(), []);
  // The filter dropdown needs every position, not the default first page:
  // the backend caps a single page at 500, so 200 keeps the dropdown
  // complete for realistic tenants (QA-002 — the old call silently
  // truncated at the default limit=50).
  const positions = useAsync(() => backend.positions.list({ limit: 200 }), []);
  const [positionFilter, setPositionFilter] = useState('');
  const [sender, setSender] = useState('Recruiter');
  const [content, setContent] = useState('');
  // C4: 补录历史消息时可以指定发生时间；留空表示"现在"（原行为）。
  const [messageTime, setMessageTime] = useState('');
  // C5: 后端 channel 是自由字符串（默认 boss，≤32 字符）；下拉给出常见值。
  const [channel, setChannel] = useState('boss');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [selectedConversationId, setSelectedConversationId] = useState<string | null>(null);

  /** Single selection entry point shared by mouse and keyboard activation. */
  function select(conversation: Conversation) {
    setSelectedConversationId(conversation.id);
  }

  const candidateMap = new Map<string, string>();
  if (candidates.data?.items) {
    for (const candidate of candidates.data.items) {
      candidateMap.set(candidate.id, candidate.name);
    }
  }

  // 岗位筛选在客户端做：会话行自带后端解析好的 position_id，
  // 无需为筛选再加一个后端参数。
  const visibleConversations = (conversations.data?.items ?? []).filter(
    (conversation) => !positionFilter || conversation.position_id === positionFilter,
  );

  const messageList = useAsync(
    () =>
      selectedConversationId
        ? backend.messages
            .list({ conversation_id: selectedConversationId })
            .then((page) => ({ conversationId: selectedConversationId, page }))
        : Promise.resolve(null),
    [selectedConversationId],
  );

  async function addMessage(event: React.FormEvent) {
    event.preventDefault();
    if (!selectedConversationId) {
      setError('请先选择候选人。');
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await backend.messages.create({
        candidate_id: conversations.data?.items.find((c) => c.id === selectedConversationId)?.candidate_id ?? '',
        conversation_id: selectedConversationId,
        sender_type: sender,
        content,
        message_time: messageTime ? new Date(messageTime).toISOString() : new Date().toISOString(),
        channel,
      });
      setContent('');
      setMessageTime('');
      setNotice('消息已记录。');
      messageList.reload();
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : '保存失败');
    } finally {
      setBusy(false);
    }
  }

  if (candidates.loading || conversations.loading) return <Loading />;
  if (candidates.error) return <ErrorBox message={candidates.error} />;
  if (conversations.error) return <ErrorBox message={conversations.error} />;

  const selectedConversation = selectedConversationId
    ? conversations.data?.items.find((c) => c.id === selectedConversationId) ?? null
    : null;

  // useAsync keeps the previous data while a new request is in flight, so the
  // timeline is only rendered when the loaded page belongs to the current
  // selection — otherwise switching conversations would show the previous
  // conversation's messages under the new header.
  const timelinePage =
    selectedConversation && messageList.data?.conversationId === selectedConversation.id
      ? messageList.data.page
      : null;

  return (
    <>
      <PageHeader
        title="沟通记录"
        description={<p>按会话查看沟通时间线，或手动补充一条记录。</p>}
        actions={
          selectedConversation ? (
            <Link className="btn" to={`/candidates/${selectedConversation.candidate_id}`}>
              打开候选人页
            </Link>
          ) : (
            <span className="muted small">请选择会话</span>
          )
        }
      />

      {error && <ErrorBox message={error} />}
      {notice && (
        <div className="notice" aria-live="polite">
          {notice}
        </div>
      )}

      <div className="grid cols-2">
        <div className="card">
          <div className="card-head">
            <h2>会话列表</h2>
          </div>
          {positions.error ? (
            <ErrorBox message={positions.error} />
          ) : (
            <div className="form-row">
              <label htmlFor="conv-position-filter">按岗位筛选</label>
              <select
                id="conv-position-filter"
                value={positionFilter}
                onChange={(event) => setPositionFilter(event.target.value)}
              >
                <option value="">全部岗位</option>
                {positions.data?.items.map((position) => (
                  <option key={position.id} value={position.id}>
                    {position.name}
                  </option>
                ))}
              </select>
            </div>
          )}
          {visibleConversations.length === 0 ? (
            <EmptyState message={positionFilter ? '该岗位下还没有会话' : '还没有会话'} />
          ) : (
            <ul className="plain">
              {visibleConversations.map((conversation) => (
                <li key={conversation.id}>
                  <button
                    type="button"
                    className="row-link"
                    onClick={() => select(conversation)}
                    aria-current={
                      selectedConversationId === conversation.id ? 'true' : undefined
                    }
                  >
                    <span className="row-link-label">
                      {candidateMap.get(conversation.candidate_id) ?? '未知候选人'}
                    </span>
                    <span className="row-link-meta">
                      {conversation.position_name ?? '—'}
                    </span>
                    <span className="row-link-meta">
                      {conversation.message_count} 条
                    </span>
                  </button>
                  {conversation.is_active && <span className="badge ok">活跃</span>}
                  <Link className="small" to={`/candidates/${conversation.candidate_id}`}>
                    候选人页
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </div>

        <div>
          {selectedConversation ? (
            <>
              <div className="card">
                <div className="card-head">
                  <h2>{conversationSummary(selectedConversation)}</h2>
                  <div className="actions">
                    <button onClick={() => setSelectedConversationId(null)}>
                      切换会话
                    </button>
                    <Link className="btn" to={`/candidates/${selectedConversation.candidate_id}`}>
                      打开候选人页
                    </Link>
                  </div>
                </div>

                <div className="card">
                  <div className="card-head">
                    <h2>手动新增消息</h2>
                  </div>
                  <form onSubmit={addMessage}>
                    <div className="form-row two">
                      <div>
                        <label htmlFor="msg-sender">发送方</label>
                        <select
                          id="msg-sender"
                          value={sender}
                          onChange={(event) => setSender(event.target.value)}
                        >
                          <option value="Recruiter">Recruiter（我方）</option>
                          <option value="Candidate">Candidate（候选人）</option>
                          <option value="System">System</option>
                        </select>
                      </div>
                      <div>
                        <label htmlFor="msg-channel">渠道</label>
                        <select
                          id="msg-channel"
                          value={channel}
                          onChange={(event) => setChannel(event.target.value)}
                        >
                          <option value="boss">BOSS 直聘</option>
                          <option value="phone">电话</option>
                          <option value="email">邮件</option>
                          <option value="wechat">微信</option>
                          <option value="other">其他</option>
                        </select>
                      </div>
                    </div>
                    <div className="form-row">
                      <label htmlFor="msg-time">时间（可选，留空记为当前时间）</label>
                      <input
                        id="msg-time"
                        type="datetime-local"
                        value={messageTime}
                        onChange={(event) => setMessageTime(event.target.value)}
                      />
                    </div>
                    <div className="form-row">
                      <label htmlFor="msg-content">内容</label>
                      <textarea
                        id="msg-content"
                        value={content}
                        onChange={(event) => setContent(event.target.value)}
                        rows={3}
                        required
                      />
                    </div>
                    <div className="actions">
                      <button className="primary" type="submit" disabled={busy}>
                        {busy ? '保存中…' : '保存'}
                      </button>
                    </div>
                  </form>
                </div>

                {messageList.error ? (
                  <PartialNotice
                    sources={[{ name: '沟通时间线', error: messageList.error, retry: messageList.reload }]}
                  />
                ) : !timelinePage ? (
                  <Loading />
                ) : timelinePage.items.length === 0 ? (
                  <EmptyState message="该会话还没有消息" />
                ) : (
                  <div className="card">
                    <div className="card-head">
                      <h2>时间线</h2>
                    </div>
                    {timelinePage.items.map((message) => (
                      <div
                        key={message.id}
                        className={`message ${message.sender_type?.toLowerCase() ?? ''}`}
                      >
                        <div className="message-head">
                          <strong>{message.sender_type ?? 'Unknown'}</strong>
                          <span>
                            <LocalTime value={message.message_time ?? message.created_at} />
                          </span>
                          {message.sender_type === 'Recruiter' && (
                            <span className="muted small">我方</span>
                          )}
                          {message.sender_type === 'Candidate' && (
                            <span className="muted small">候选人</span>
                          )}
                        </div>
                        <div className="message-body">{message.content}</div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </>
          ) : (
            <EmptyState message="选择一个会话查看时间线" />
          )}
        </div>
      </div>
    </>
  );
}

function conversationSummary(conversation: Conversation): string {
  const parts = [conversation.title ?? '会话'];
  parts.push(`${conversation.message_count} 条消息`);
  if (conversation.last_message_at) {
    parts.push(`最后活跃：${conversation.last_message_at}`);
  }
  return parts.join(' · ');
}
