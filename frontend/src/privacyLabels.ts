/**
 * Human labels for the privacy categories the backend reports in
 * ``redaction_applied`` / ``masked_fields``.
 *
 * Kept in one place so the candidate page's 「已脱敏字段」 hint and the
 * settings page's disclosure list can never drift apart, and so an unknown
 * key falls back to the raw key instead of disappearing from the UI.
 */
const REDACTION_LABELS: Record<string, string> = {
  name: '姓名（以 [NAME] 占位）',
  phone: '手机号',
  email: '邮箱',
  id_number: '身份证号',
  card_number: '银行卡号',
};

export function redactionLabel(key: string): string {
  return REDACTION_LABELS[key] ?? key;
}
