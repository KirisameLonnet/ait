//! Shared keyset cursor encoding; capability crates own their fields and default ordering.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};

use crate::ErrorCode;

mod collation;

/// Scalar values accepted by Paseo's directory cursor codec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SortValue {
    /// Normalized, case-insensitive text.
    Text(String),
    /// Numeric priority or epoch milliseconds.
    Number(i64),
    /// Missing values sort before present values in ascending order.
    Null,
}

/// Direction of one sort clause.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Ascending order.
    Asc,
    /// Descending order.
    Desc,
}

/// A capability-owned field name and its comparison direction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sort {
    /// Wire field name.
    pub key: String,
    /// Ordering of this field.
    pub direction: Direction,
}

/// A row with precomputed sort values; sorting never clones the payload.
#[derive(Debug)]
pub struct Entry<T> {
    /// Stable identity used to break ties independently of sort direction.
    pub id: String,
    /// Only fields named in the sort clauses are encoded into cursors.
    pub values: BTreeMap<String, SortValue>,
    /// Capability-owned row.
    pub value: T,
}

/// A bounded page and its opaque continuation tokens.
#[derive(Debug)]
pub struct Page<T> {
    /// Rows after the supplied cursor.
    pub entries: Vec<T>,
    /// Continuation after the last returned row, when more rows exist.
    pub next_cursor: Option<String>,
    /// The input cursor, matching Paseo's previous-page metadata.
    pub prev_cursor: Option<String>,
    /// Whether another matching row follows this page.
    pub has_more: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct Cursor {
    sort: Vec<Sort>,
    values: BTreeMap<String, SortValue>,
    id: String,
}

/// Sort `entries` by `sort`, then identity, and return up to `limit` rows after `token`.
///
/// Repeated sort keys keep their first occurrence. Capability crates must supply their default
/// sort when no clauses were requested and precompute the matching scalar values on each row.
/// Removing an earlier row does not change which rows follow a previously issued cursor.
///
/// # Errors
/// Returns `InvalidMessage` for a limit outside 1..=200, an empty sort, or a malformed,
/// oversized, or differently sorted cursor. Encoding errors also return `InvalidMessage`.
pub fn paginate<T>(
    mut entries: Vec<Entry<T>>,
    sort: &[Sort],
    limit: usize,
    token: Option<&str>,
) -> Result<Page<T>, ErrorCode> {
    if !(1..=200).contains(&limit) || sort.is_empty() {
        return Err(ErrorCode::InvalidMessage);
    }
    let mut normalized = Vec::with_capacity(sort.len());
    for clause in sort {
        if !normalized
            .iter()
            .any(|entry: &Sort| entry.key == clause.key)
        {
            normalized.push(clause.clone());
        }
    }
    let sort = normalized;
    let cursor = token
        .filter(|token| !token.is_empty())
        .map(|token| decode(token, &sort))
        .transpose()?;
    entries.sort_by(|left, right| {
        compare(&left.values, &right.values, &sort)
            .then_with(|| collation::compare(&left.id, &right.id))
    });
    let mut selected = entries
        .into_iter()
        .filter(|entry| {
            cursor.as_ref().is_none_or(|cursor| {
                compare(&entry.values, &cursor.values, &sort)
                    .then_with(|| collation::compare(&entry.id, &cursor.id))
                    == Ordering::Greater
            })
        })
        .take(limit + 1)
        .collect::<Vec<_>>();
    let has_more = selected.len() > limit;
    selected.truncate(limit);
    let next_cursor = if has_more {
        selected
            .last()
            .map(|entry| encode(entry, &sort))
            .transpose()?
    } else {
        None
    };
    Ok(Page {
        entries: selected.into_iter().map(|entry| entry.value).collect(),
        next_cursor,
        prev_cursor: token.map(str::to_owned),
        has_more,
    })
}

fn compare(
    left: &BTreeMap<String, SortValue>,
    right: &BTreeMap<String, SortValue>,
    sort: &[Sort],
) -> Ordering {
    sort.iter()
        .map(|sort| {
            let left = left.get(&sort.key).unwrap_or(&SortValue::Null);
            let right = right.get(&sort.key).unwrap_or(&SortValue::Null);
            let order = match (left, right) {
                (SortValue::Null, SortValue::Null) => Ordering::Equal,
                (SortValue::Null, _) => Ordering::Less,
                (_, SortValue::Null) => Ordering::Greater,
                (SortValue::Text(left), SortValue::Text(right)) => collation::compare(left, right),
                (SortValue::Number(left), SortValue::Number(right)) => left.cmp(right),
                (SortValue::Number(left), SortValue::Text(right)) => {
                    collation::compare(&left.to_string(), right)
                }
                (SortValue::Text(left), SortValue::Number(right)) => {
                    collation::compare(left, &right.to_string())
                }
            };
            match sort.direction {
                Direction::Asc => order,
                Direction::Desc => order.reverse(),
            }
        })
        .find(|order| *order != Ordering::Equal)
        .unwrap_or(Ordering::Equal)
}

fn decode(token: &str, sort: &[Sort]) -> Result<Cursor, ErrorCode> {
    if token.len() > 16 * 1024 {
        return Err(ErrorCode::InvalidMessage);
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| ErrorCode::InvalidMessage)?;
    let cursor: Cursor = serde_json::from_slice(&bytes).map_err(|_| ErrorCode::InvalidMessage)?;
    if cursor.sort != sort {
        return Err(ErrorCode::InvalidMessage);
    }
    Ok(cursor)
}

fn encode<T>(entry: &Entry<T>, sort: &[Sort]) -> Result<String, ErrorCode> {
    let cursor = Cursor {
        sort: sort.to_vec(),
        values: sort
            .iter()
            .map(|sort| {
                (
                    sort.key.clone(),
                    entry
                        .values
                        .get(&sort.key)
                        .cloned()
                        .unwrap_or(SortValue::Null),
                )
            })
            .collect(),
        id: entry.id.clone(),
    };
    serde_json::to_vec(&cursor)
        .map(|bytes| URL_SAFE_NO_PAD.encode(bytes))
        .map_err(|_| ErrorCode::InvalidMessage)
}

#[cfg(test)]
mod tests;
