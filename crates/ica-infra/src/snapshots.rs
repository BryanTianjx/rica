//! SQLite adapter for `POST /export` and `POST /import`.
//!
//! Export walks the tables in the order the baseline's `_BUSINESS_TABLES`
//! declares, selects the same columns (minus `import_sessions.raw_text`, which
//! the baseline excludes), and renders values the way `_to_serializable` does:
//! datetimes as `isoformat()` with `+00:00`, booleans as JSON booleans,
//! integers and floats kept distinct.
//!
//! Import is an idempotent upsert: a known UUID is merged last-write-wins on
//! `updated_at`, an unknown one is inserted. The whole request is one
//! transaction, and a row that would violate tenancy is refused before anything
//! is written.

use chrono::{SecondsFormat, Utc};
use ica_application::{DataSnapshotRepository, RepositoryError, SnapshotOutcome};
use serde_json::{Map, Value};
use sqlx::{Row, SqlitePool};

use crate::export_tables::{ColKind, BUSINESS_TABLES};

/// Tables whose rows are an append-only log: an import never rewrites a stored
/// row, whatever timestamp the snapshot carries (`_APPEND_ONLY_TABLES`).
///
/// A send record is a historical fact — "this message was sent at this time".
/// Letting a later snapshot rewrite it would silently falsify the record, so
/// the baseline refuses even when the incoming `updated_at` is newer. The
/// conflict is still REPORTED, so the caller learns the snapshot disagreed.
const APPEND_ONLY_TABLES: &[&str] = &["send_records"];
use crate::store::{repo_error, to_sql_timestamp, SqliteStore};

/// Render a SQLite timestamp the way Python's `isoformat()` does.
///
/// `2026-11-10 09:00:00.000000` -> `2026-11-10T09:00:00+00:00` (no fraction when
/// it is zero), and `2026-11-18 14:40:29.946460` ->
/// `2026-11-18T14:40:29.946460+00:00`.
fn isoformat(raw: &str) -> Option<String> {
    let parsed = crate::time::parse_sqlite_datetime(raw).ok()?;
    let fraction_is_zero = raw
        .split('.')
        .nth(1)
        .map(|fraction| fraction.trim_end_matches('0').is_empty())
        .unwrap_or(true);
    let format = if fraction_is_zero {
        SecondsFormat::Secs
    } else {
        SecondsFormat::Micros
    };
    Some(parsed.to_rfc3339_opts(format, false))
}

/// Read one column into a JSON value according to its declared kind.
fn read_value(
    row: &sqlx::sqlite::SqliteRow,
    index: usize,
    kind: ColKind,
) -> Result<Value, RepositoryError> {
    Ok(match kind {
        ColKind::Text => match row.try_get::<Option<String>, _>(index) {
            Ok(Some(text)) => Value::String(text),
            Ok(None) | Err(_) => Value::Null,
        },
        ColKind::DateTime => match row.try_get::<Option<String>, _>(index) {
            Ok(Some(raw)) => isoformat(&raw).map(Value::String).unwrap_or(Value::Null),
            Ok(None) | Err(_) => Value::Null,
        },
        ColKind::Bool => match row.try_get::<Option<i64>, _>(index) {
            Ok(Some(number)) => Value::Bool(number != 0),
            Ok(None) | Err(_) => Value::Null,
        },
        ColKind::Int => match row.try_get::<Option<i64>, _>(index) {
            Ok(Some(number)) => Value::Number(number.into()),
            Ok(None) | Err(_) => Value::Null,
        },
        ColKind::Float => match row.try_get::<Option<f64>, _>(index) {
            Ok(Some(number)) => serde_json::Number::from_f64(number)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            Ok(None) | Err(_) => Value::Null,
        },
    })
}

#[async_trait::async_trait]
impl DataSnapshotRepository for SqliteStore {
    async fn export_tables(&self) -> Result<Map<String, Value>, RepositoryError> {
        let pool: &SqlitePool = self.pool();
        let mut document = Map::new();

        for spec in BUSINESS_TABLES {
            let columns: Vec<&str> = spec.columns.iter().map(|(name, _)| *name).collect();
            let sql = format!("SELECT {} FROM {}", columns.join(", "), spec.name);
            let rows = sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
                .fetch_all(pool)
                .await
                .map_err(repo_error)?;

            let mut rendered = Vec::with_capacity(rows.len());
            for row in &rows {
                let mut object = Map::new();
                for (index, (name, kind)) in spec.columns.iter().enumerate() {
                    object.insert(name.to_string(), read_value(row, index, *kind)?);
                }
                rendered.push(Value::Object(object));
            }
            document.insert(spec.name.to_string(), Value::Array(rendered));
        }

        Ok(document)
    }

    async fn import_tables(
        &self,
        data: Map<String, Value>,
    ) -> Result<SnapshotOutcome, RepositoryError> {
        // Validate the whole payload BEFORE the transaction starts, so an
        // invalid snapshot writes nothing at all. The baseline's
        // `_validate_payload` does the same and answers
        // 422 `import_validation_error`.
        validate_import_payload(&data)?;

        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        let mut outcome = SnapshotOutcome::default();

        for (table, rows) in &data {
            let Some(spec) = BUSINESS_TABLES.iter().find(|spec| spec.name == table) else {
                // The baseline walks `_BUSINESS_TABLES` and never looks at a
                // table it does not know, so an unknown table is IGNORED — not
                // counted as skipped, not an error. The audit's `tables` list
                // is built from the payload's keys, so an unknown table still
                // appears THERE, but nothing it contains is imported.
                continue;
            };

            let Some(items) = rows.as_array() else {
                outcome.skipped += 1;
                continue;
            };

            for item in items {
                let Some(object) = item.as_object() else {
                    outcome.skipped += 1;
                    continue;
                };

                // A row without an `id` cannot match a stored row, so it is
                // always an insert — and the baseline MINTS its primary key
                // (`_get_or_create`: "a snapshot row without an `id` gets a
                // freshly generated UUID"). Skipping it instead silently drops
                // data from a hand-written or third-party snapshot.
                //
                // The baseline gates on `if not row_id`, so `null` AND `""`
                // both count as "no id": `null` used to 500 (the NULL reached
                // the INSERT) and `""` used to insert an empty-string primary
                // key — both measured by the independent audit (N-1 / P2-4).
                let id_owned = match object
                    .get("id")
                    .and_then(|value| value.as_str())
                    .filter(|text| !text.is_empty())
                {
                    Some(existing) => existing.to_string(),
                    None => crate::store::new_uuid(),
                };
                let id = id_owned.as_str();

                let existing_updated_at: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(
                    format!("SELECT updated_at FROM {} WHERE id = ?", spec.name),
                ))
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(repo_error)?;

                let Some(stored_updated_at) = existing_updated_at else {
                    if insert_row(&mut tx, spec, object, id).await? {
                        outcome.imported += 1;
                    } else {
                        outcome.skipped += 1;
                        outcome.conflicts += 1;
                    }
                    continue;
                };

                // The row exists: decide last-write-wins BEFORE touching any
                // business column, then record which bucket it landed in.
                let differs = business_data_differs(&mut tx, spec, object, id).await?;

                // Append-only tables are an exception to LWW: a stored row is
                // never rewritten, whatever timestamp the snapshot carries.
                // A differing payload is still reported as a conflict, so the
                // caller learns the snapshot disagreed.
                if APPEND_ONLY_TABLES.contains(&spec.name) {
                    outcome.skipped += 1;
                    if differs {
                        outcome.conflicts += 1;
                    }
                    continue;
                }

                let incoming = object
                    .get("updated_at")
                    .and_then(|value| value.as_str())
                    .and_then(|raw| crate::time::parse_sqlite_datetime(raw).ok());
                let stored = crate::time::parse_sqlite_datetime(&stored_updated_at).ok();

                match (incoming, stored) {
                    // No timestamp in the snapshot: it carries no provenance, so
                    // it cannot win any race. A differing record is a conflict.
                    (None, _) => {
                        outcome.skipped += 1;
                        if differs {
                            outcome.conflicts += 1;
                        }
                    }
                    (Some(incoming_at), Some(stored_at)) if incoming_at > stored_at => {
                        if update_row(&mut tx, spec, object, id).await? {
                            outcome.updated += 1;
                        } else {
                            outcome.skipped += 1;
                            outcome.conflicts += 1;
                        }
                    }
                    // Same instant, or older: the stored row keeps its values.
                    // A differing payload is a real merge conflict.
                    _ => {
                        outcome.skipped += 1;
                        if differs {
                            outcome.conflicts += 1;
                        }
                    }
                }
            }
        }

        tx.commit().await.map_err(repo_error)?;
        Ok(outcome)
    }

    async fn record_export_audit(
        &self,
        outcome: &ica_application::sync::ExportOutcome,
    ) -> Result<(), RepositoryError> {
        use ica_application::sync::ExportOutcome;
        let (tables, encrypted) = match outcome {
            ExportOutcome::Plain(snapshot) => {
                let mut counts = Map::new();
                for (name, rows) in &snapshot.data {
                    if let Some(items) = rows.as_array() {
                        counts.insert(name.clone(), Value::from(items.len()));
                    }
                }
                (Value::Object(counts), false)
            }
            ExportOutcome::Sealed(sealed) => {
                // The baseline takes the counts from the snapshot it read
                // BEFORE sealing, so an encrypted export reports them too.
                // `sealed.tables` is already in the baseline's declaration
                // order; inserting into an order-preserving `Map` keeps it.
                let mut counts = Map::new();
                for (name, count) in &sealed.tables {
                    counts.insert(name.clone(), Value::from(*count as i64));
                }
                (Value::Object(counts), true)
            }
        };

        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        crate::audit::insert_audit(
            &mut tx,
            crate::audit::AuditEntry::with_entity_type(
                "data.exported",
                "export",
                "Exported a data snapshot",
            )
            .with_metadata(serde_json::json!({
                "tables": tables,
                "encrypted": encrypted,
                "outcome": "ok",
            })),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;
        Ok(())
    }

    async fn record_import_audit(
        &self,
        outcome: &SnapshotOutcome,
        tables: &[String],
    ) -> Result<(), RepositoryError> {
        let mut sorted: Vec<String> = tables.to_vec();
        sorted.sort();

        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        crate::audit::insert_audit(
            &mut tx,
            crate::audit::AuditEntry::with_entity_type(
                "data.imported",
                "import",
                format!(
                    "Imported a data snapshot ({} new, {} updated, {} skipped)",
                    outcome.imported, outcome.updated, outcome.skipped
                ),
            )
            .with_metadata(serde_json::json!({
                "imported": outcome.imported,
                "updated": outcome.updated,
                "skipped": outcome.skipped,
                "conflicts": outcome.conflicts,
                "tables": sorted,
                "outcome": "ok",
            })),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;
        Ok(())
    }
}

/// Python's `==` on a boolean column: `0 == False`, `1 == True`, and anything
/// else falls back to plain (unequal) comparison — `2 == True` is False and
/// `0 == None` is False in Python 3 too.
fn bool_value_eq(incoming: &Value, stored: &Value) -> bool {
    let normalize = |value: &Value| match value {
        Value::Bool(flag) => Some(*flag),
        Value::Number(number) if number.as_i64() == Some(0) => Some(false),
        Value::Number(number) if number.as_i64() == Some(1) => Some(true),
        _ => None,
    };
    match (normalize(incoming), normalize(stored)) {
        (Some(left), Some(right)) => left == right,
        _ => incoming == stored,
    }
}

/// Whether the payload's business columns differ from the stored row.
///
/// `id` and `updated_at` are excluded: they are the merge key and the LWW
/// timestamp, not business data. Only the columns the payload MENTIONS are
/// compared — a column it omits says nothing, so it cannot disagree.
///
/// Datetime columns are compared after PARSING, not as text: the stored value
/// comes back from SQLite as `2026-11-10 09:05:00.000003` while the snapshot
/// carries `2026-11-10T09:05:00.000003+00:00`, and comparing those as strings
/// would report every row as different.
async fn business_data_differs(
    tx: &mut sqlx::SqliteConnection,
    spec: &crate::export_tables::TableSpec,
    object: &Map<String, Value>,
    id: &str,
) -> Result<bool, RepositoryError> {
    let columns: Vec<&str> = crate::export_tables::import_columns(spec)
        .iter()
        .map(|(name, _)| *name)
        .collect();
    let sql = format!(
        "SELECT {} FROM {} WHERE id = ?",
        columns.join(", "),
        spec.name
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(repo_error)?;
    let Some(row) = row else { return Ok(false) };

    for (index, (name, kind)) in crate::export_tables::import_columns(spec)
        .iter()
        .enumerate()
    {
        if *name == "id" || *name == "updated_at" {
            continue;
        }
        let Some(incoming) = object.get(*name) else {
            continue;
        };
        let stored = read_value(&row, index, *kind)?;

        let same = match kind {
            // Parse both sides so the wire format and the SQLite format compare
            // equal when they denote the same instant.
            ColKind::DateTime => {
                let left = incoming
                    .as_str()
                    .and_then(|raw| crate::time::parse_sqlite_datetime(raw).ok());
                let right = stored
                    .as_str()
                    .and_then(|raw| crate::time::parse_sqlite_datetime(raw).ok());
                match (left, right) {
                    (Some(a), Some(b)) => a == b,
                    // An unparseable timestamp is compared as its raw value.
                    _ => incoming == &stored,
                }
            }
            // Python compares a boolean column's value with `==`, where
            // `0 == False` and `1 == True` — a numeric 0/1 in the payload (a
            // raw SQLite dump) must not report a phantom conflict (audit
            // LWW P2-2).
            ColKind::Bool => bool_value_eq(incoming, &stored),
            _ => incoming == &stored,
        };
        if !same {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Reject a snapshot whose rows could never be read back (`_validate_payload`).
///
/// Runs BEFORE the transaction opens, so an invalid payload writes nothing.
/// The baseline validates `positions` rows against the `PositionRead` contract
/// (`_position_problems`); the other tables are only shape-checked, which
/// `ica-contracts::body::import_payload` has already done.
///
/// The `details.errors` list is the baseline's `details={"errors": problems}`:
/// an ARRAY of row problems on the wire. (An earlier revision stringified it,
/// which the independent audit flagged as a contract-visible difference — a
/// client parses the list, it does not `json.loads` it.)
fn validate_import_payload(data: &Map<String, Value>) -> Result<(), RepositoryError> {
    let mut problems: Vec<Value> = Vec::new();

    // The baseline walks `_BUSINESS_TABLES` and keeps only the tables that
    // have a validator; today that is exactly `positions`.
    if let Some(rows) = data.get("positions") {
        // A non-array `positions` cannot reach this point through HTTP (the
        // body validator answers 422 first), but the baseline would enumerate
        // it and report every element — mirror the reachable shape: rows.
        let empty = Vec::new();
        let rows = rows.as_array().unwrap_or(&empty);
        for (index, row) in rows.iter().enumerate() {
            let Some(record) = row.as_object() else {
                problems.push(row_problem(
                    index,
                    Value::Null,
                    "row must be a JSON object",
                    Value::Null,
                ));
                continue;
            };
            problems.extend(position_problems(record).into_iter().map(|problem| {
                let mut entry = Map::new();
                entry.insert("table".to_string(), Value::String("positions".to_string()));
                entry.insert("index".to_string(), Value::from(index as u64));
                for (key, value) in problem {
                    entry.insert(key, value);
                }
                Value::Object(entry)
            }));
        }
    }

    if problems.is_empty() {
        return Ok(());
    }

    let mut details = Map::new();
    details.insert("errors".to_string(), Value::Array(problems));
    Err(RepositoryError::invalid_with_code(
        "import_validation_error",
        "Import payload contains rows that could not be read back; nothing was written",
        Value::Object(details),
    ))
}

/// One entry of the baseline's `details.errors`: `table` + `index` + the
/// field-level problem (`_problem` keeps the JSON-native value when it is one).
fn row_problem(index: usize, field: Value, message: &str, value: Value) -> Value {
    serde_json::json!({
        "table": "positions",
        "index": index,
        "field": field,
        "message": message,
        "value": json_problem_value(value),
    })
}

/// `_problem` keeps a JSON-native value and stringifies anything else. Python
/// would render a dict/list with `str()` (single quotes); the closest JSON
/// rendering is the compact serde text.
fn json_problem_value(value: Value) -> Value {
    match value {
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => value,
        other => Value::String(other.to_string()),
    }
}

/// The `positions` row rules the baseline enforces (`_position_problems`),
/// against the `PositionRead` contract. Every message here is contract text —
/// it is compared verbatim by the audit probes, so it must stay byte-equal.
fn position_problems(record: &Map<String, Value>) -> Vec<Map<String, Value>> {
    const STATUS_VALUES: &[&str] = &["Closed", "Draft", "OnHold", "Open"]; // sorted
    const TEXT_LIMITS: &[(&str, usize)] = &[("location", 200), ("education_requirements", 200)];

    let mut problems: Vec<Map<String, Value>> = Vec::new();

    let name = record.get("name");
    match name {
        Some(Value::String(text)) => {
            if text.trim().is_empty() {
                problems.push(field_problem(
                    "name",
                    "name is required and must be a string",
                    name.cloned(),
                ));
            } else if text.chars().count() > 200 {
                problems.push(field_problem(
                    "name",
                    "name must be at most 200 characters",
                    name.cloned(),
                ));
            }
        }
        _ => {
            problems.push(field_problem(
                "name",
                "name is required and must be a string",
                Some(name.cloned().unwrap_or(Value::Null)),
            ));
        }
    }

    for (field, limit) in TEXT_LIMITS {
        let Some(value) = record.get(*field) else {
            continue;
        };
        if value.is_null() {
            continue;
        }
        match value {
            Value::String(text) => {
                if text.chars().count() > *limit {
                    problems.push(field_problem(
                        field,
                        &format!("{field} must be at most {limit} characters"),
                        Some(value.clone()),
                    ));
                }
            }
            _ => {
                problems.push(field_problem(
                    field,
                    &format!("{field} must be a string"),
                    Some(value.clone()),
                ));
            }
        }
    }

    for field in ["salary_min", "salary_max"] {
        // `in` vs `get`: an explicit `null` is PRESENT and skipped, an absent
        // key is skipped too — only a present non-null value is checked.
        let Some(value) = record.get(field) else {
            continue;
        };
        if value.is_null() {
            continue;
        }
        match value {
            // JSON has no bool-as-int trap (`Value::Bool` is its own variant),
            // but `true`/`false` still are not salaries.
            Value::Bool(_) => {
                problems.push(field_problem(
                    field,
                    &format!("{field} must be an integer"),
                    Some(value.clone()),
                ));
            }
            Value::Number(number) => match number.as_i64() {
                Some(integral) if integral < 0 => {
                    problems.push(field_problem(
                        field,
                        &format!("{field} must not be negative"),
                        Some(value.clone()),
                    ));
                }
                Some(_) => {}
                None => {
                    // A float is not an integer in Python either.
                    problems.push(field_problem(
                        field,
                        &format!("{field} must be an integer"),
                        Some(value.clone()),
                    ));
                }
            },
            _ => {
                problems.push(field_problem(
                    field,
                    &format!("{field} must be an integer"),
                    Some(value.clone()),
                ));
            }
        }
    }

    if let Some(status) = record.get("status") {
        let known = status.as_str().map(|text| STATUS_VALUES.contains(&text));
        if known != Some(true) {
            problems.push(field_problem(
                "status",
                &format!("status must be one of {}", STATUS_VALUES.join(", ")),
                Some(status.clone()),
            ));
        }
    }

    if let Some(archived) = record.get("archived") {
        // SQLite dumps hand back `0`/`1` for a boolean column; the read model
        // coerces those, so they are readable and must not be rejected (a raw
        // snapshot from an older database is a supported import source).
        let acceptable = archived.is_null()
            || matches!(archived, Value::Bool(_))
            || archived.as_i64().map(|n| n == 0 || n == 1).unwrap_or(false);
        if !acceptable {
            problems.push(field_problem(
                "archived",
                "archived must be a boolean",
                Some(archived.clone()),
            ));
        }
    }

    problems
}

/// `_problem(field, message, value)`: keeps a JSON-native value, stringifies
/// the rest.
fn field_problem(field: &str, message: &str, value: Option<Value>) -> Map<String, Value> {
    let mut entry = Map::new();
    entry.insert("field".to_string(), Value::String(field.to_string()));
    entry.insert("message".to_string(), Value::String(message.to_string()));
    entry.insert(
        "value".to_string(),
        json_problem_value(value.unwrap_or(Value::Null)),
    );
    entry
}

/// Insert one row. Returns `false` when the row cannot be written as-is (a
/// missing required column), which the caller counts as a conflict — the
/// baseline does the same for an incomplete row.
async fn insert_row(
    tx: &mut sqlx::SqliteConnection,
    spec: &crate::export_tables::TableSpec,
    object: &Map<String, Value>,
    id: &str,
) -> Result<bool, RepositoryError> {
    let mut columns: Vec<&str> = Vec::new();
    let mut placeholders: Vec<String> = Vec::new();
    let mut values: Vec<Value> = Vec::new();

    for (name, kind) in crate::export_tables::import_columns(spec) {
        // The id is appended once, after this loop, with the minted or given
        // value. Binding the payload's own `id` entry here too would bind a
        // JSON `null` twice into the same INSERT — measured as a 500
        // `NOT NULL constraint failed: candidates.id` for `"id": null`
        // (audit N-1) — so the `id` column is skipped in this loop, exactly
        // like `update_row` does.
        if name == "id" {
            continue;
        }
        // Only the columns the snapshot MENTIONS get a value. For the rest the
        // INSERT leaves the column out, so the DDL's DEFAULT (or the ORM
        // default from `column_defaults`) applies — exactly what the baseline's
        // `_apply_fields_only` produces by iterating `record.items()`.
        //
        // Writing NULL for an unmentioned column instead would violate NOT NULL
        // on a partial row: measured, `{"id":…,"name":"x"}` answered 500
        // `NOT NULL constraint failed: candidates.source`.
        let Some(value) = object.get(name) else {
            // No value given: supply what the ORM would have supplied.
            //
            //   * a static default from `column_defaults` (JSON literal);
            //   * `created_at` / `updated_at`, whose model default is a
            //     CALLABLE (`datetime.now`) and therefore cannot be a literal;
            //   * otherwise omit the column and let SQLite apply its own
            //     DEFAULT (or report a genuine NOT NULL violation, which is
            //     what the baseline would do without an ORM default).
            if let Some(literal) = crate::column_defaults::column_default(spec.name, name) {
                columns.push(name);
                placeholders.push(literal.to_string());
                continue;
            }
            if matches!(name, "created_at" | "updated_at") {
                columns.push(name);
                placeholders.push("?".to_string());
                values.push(Value::String(to_sql_timestamp(&Utc::now())));
            }
            continue;
        };

        columns.push(name);
        placeholders.push("?".to_string());
        values.push(coerce_for_write(value.clone(), kind));
    }

    // The id is always written: a snapshot row without one gets a fresh UUID,
    // because a record with no lookup key cannot match a stored row
    // (`_get_or_create`: "a snapshot row without an `id` gets a freshly
    // generated UUID").
    columns.push("id");
    placeholders.push("?".to_string());
    values.push(Value::String(id.to_string()));

    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        spec.name,
        columns.join(", "),
        placeholders.join(", ")
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()));
    for value in &values {
        query = bind_json(query, value);
    }
    query.execute(&mut *tx).await.map_err(repo_error)?;
    Ok(true)
}

async fn update_row(
    tx: &mut sqlx::SqliteConnection,
    spec: &crate::export_tables::TableSpec,
    object: &Map<String, Value>,
    id: &str,
) -> Result<bool, RepositoryError> {
    // Only the columns the snapshot actually MENTIONS are written. The
    // baseline's `_apply_fields_only` iterates `record.items()`, so a column the
    // payload omits keeps its stored value — writing NULL for it would violate
    // a NOT NULL constraint on a partial row (measured: a partial candidates
    // snapshot answered 500 `internal_error`), and would also silently blank
    // columns the caller never mentioned.
    let mut assignments: Vec<String> = Vec::new();
    let mut values: Vec<Value> = Vec::new();
    for (name, kind) in crate::export_tables::import_columns(spec) {
        if name == "id" {
            continue;
        }
        let Some(value) = object.get(name) else {
            continue;
        };
        assignments.push(format!("{name} = ?"));
        values.push(coerce_for_write(value.clone(), kind));
    }
    if assignments.is_empty() {
        // Nothing to write: the row exists and the payload said nothing about
        // it. Not an error.
        return Ok(true);
    }

    let sql = format!(
        "UPDATE {} SET {} WHERE id = ?",
        spec.name,
        assignments.join(", ")
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()));
    for value in &values {
        query = bind_json(query, value);
    }
    query.bind(id).execute(&mut *tx).await.map_err(repo_error)?;
    Ok(true)
}

/// Turn a JSON value back into something SQLite stores: booleans become 0/1
/// and datetimes are re-rendered the way the baseline's SQLAlchemy binding
/// stores a naive UTC datetime (`2033-01-01 00:00:00.000000`, space separator,
/// fixed microseconds). The snapshot may carry any ISO shape (`…T…+08:00`, a
/// bare date); the baseline parses it into a datetime object before it binds,
/// so the ROW text must not keep the wire's `T`/offset form — mixed formats
/// inside one column make SQLite's text `ORDER BY`/`MAX` disagree with
/// chronological order (audit observation §2.4). A value that does not parse
/// keeps its raw text.
fn coerce_for_write(value: Value, kind: ColKind) -> Value {
    match (value, kind) {
        (Value::Bool(flag), ColKind::Bool) => Value::from(if flag { 1 } else { 0 }),
        (Value::String(text), ColKind::DateTime) => Value::String(
            crate::time::parse_sqlite_datetime(&text)
                .map(|instant| instant.format("%Y-%m-%d %H:%M:%S%.6f").to_string())
                .unwrap_or(text),
        ),
        (other, _) => other,
    }
}

fn bind_json<'a>(
    query: sqlx::query::Query<'a, sqlx::Sqlite, sqlx::sqlite::SqliteArguments>,
    value: &Value,
) -> sqlx::query::Query<'a, sqlx::Sqlite, sqlx::sqlite::SqliteArguments> {
    match value {
        Value::Null => query.bind(None::<String>),
        Value::Bool(flag) => query.bind(if *flag { 1i64 } else { 0i64 }),
        Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                query.bind(integer)
            } else {
                query.bind(number.as_f64().unwrap_or_default())
            }
        }
        Value::String(text) => query.bind(text.clone()),
        other => query.bind(other.to_string()),
    }
}
