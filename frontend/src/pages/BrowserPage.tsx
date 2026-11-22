/**
 * /browser 页 —— 浏览器子系统全部 6 个面板的组装（BROWSER-05 前端交付）。
 *
 * 页面本身保持薄：只负责运行时状态的轮询与面板编排，业务交互都在
 * src/components/browser/* 的面板里。runtime status 由页面拉一次并下发给
 * 运行时 / 会话两个面板（会话台需要 ready 状态与会话上限）。
 */

import { useAsync } from '../hooks/useAsync';
import { backend } from '../services/backend';
import { ErrorBox } from '../components/ui';
import { RuntimePanel } from '../components/browser/RuntimePanel';
import { SessionPanel } from '../components/browser/SessionPanel';
import { PermissionsPanel } from '../components/browser/PermissionsPanel';
import { ApprovalsPanel } from '../components/browser/ApprovalsPanel';
import { TasksPanel } from '../components/browser/TasksPanel';
import { ExtractionsPanel } from '../components/browser/ExtractionsPanel';

export function BrowserPage() {
  // 页面级拉一次运行时状态，下发给会话台（ready 状态 / 会话上限）。
  // 运行时面板内部自己持有一份（自检 / 启停后各自 reload）。
  // 仅用于页头的错误呈现；会话台与运行时面板各自拉取更细的状态。
  const runtime = useAsync(() => backend.browser.runtimeStatus(), []);

  return (
    <div>
      <div className="page-head">
        <h1>浏览器自动化</h1>
        <p>
          运行时、会话、权限、审批、AI 任务与候选人提取。未授权域名的访问会被拒绝，
          不可逆操作必须人工审批。
        </p>
      </div>

      {runtime.error && <ErrorBox message={`运行时状态读取失败：${runtime.error}`} />}

      <section className="card">
        <div className="card-head">
          <h2>运行时</h2>
        </div>
        <RuntimePanel />
      </section>

      <section className="card">
        <div className="card-head">
          <h2>会话操作台</h2>
        </div>
        <SessionPanel />
      </section>

      <section className="card">
        <div className="card-head">
          <h2>权限</h2>
        </div>
        <PermissionsPanel />
      </section>

      <section className="card">
        <div className="card-head">
          <h2>审批</h2>
        </div>
        <ApprovalsPanel />
      </section>

      <section className="card">
        <div className="card-head">
          <h2>任务</h2>
        </div>
        <TasksPanel />
      </section>

      <section className="card">
        <div className="card-head">
          <h2>候选人提取</h2>
        </div>
        <ExtractionsPanel />
      </section>
    </div>
  );
}
