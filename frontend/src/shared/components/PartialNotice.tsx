/** Phase 1 WP09 — one consistent place for "some data failed to load".
 * The page's primary content stays usable; each failed secondary source is
 * named so the user knows what is unavailable and can retry that source.
 */
export interface PartialSource {
  name: string;
  error: string | null;
  retry?: () => void;
}

export function PartialNotice({ sources }: { sources: PartialSource[] }) {
  const failed = sources.filter((source) => source.error);
  if (failed.length === 0) return null;
  return (
    <div className="partial-notice" role="alert">
      <strong>部分数据不可用</strong>
      <ul>
        {failed.map((source) => (
          <li key={source.name}>
            {source.name}：{source.error}
            {source.retry && (
              <button type="button" className="link-btn" onClick={source.retry}>
                重试
              </button>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}
