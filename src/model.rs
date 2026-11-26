// Copyright © The Daybrite Project
// SPDX-License-Identifier: GPL-2.0-or-later
//! A planner object has a stable identity. Each editable field is a causal multi-value
//! register so editing a note never writes back yesterday's completion state.
use day::datetime::DayDate;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Task,
    Note,
    List,
    Group,
    Step,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Register {
    // A live candidate for actor A has dot (A, seen[A]). Missing candidates with an
    // observed dot have been superseded. Context survives even after conflict resolution.
    seen: BTreeMap<String, u64>,
    tips: BTreeMap<String, String>,
}

impl Register {
    fn set(&mut self, actor: &str, value: String) -> Result<(), Invalid> {
        let clock = self
            .seen
            .values()
            .max()
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(Invalid)?;
        self.seen.insert(actor.into(), clock);
        self.tips.clear();
        self.tips.insert(actor.into(), value);
        Ok(())
    }

    fn join(&self, other: &Self) -> Result<Self, Invalid> {
        let mut joined = Self {
            seen: self.seen.clone(),
            tips: BTreeMap::new(),
        };
        for (actor, &clock) in &other.seen {
            let entry = joined.seen.entry(actor.clone()).or_default();
            *entry = (*entry).max(clock);
        }
        for (a, b) in [(self, other), (other, self)] {
            for (actor, value) in &a.tips {
                let clock = a.seen.get(actor).ok_or(Invalid)?;
                match b.seen.get(actor) {
                    Some(c) if c > clock => continue,
                    Some(c) if c == clock => match b.tips.get(actor) {
                        None => continue,
                        Some(v) if v != value => return Err(Invalid),
                        _ => {}
                    },
                    _ => {}
                }
                joined.tips.insert(actor.clone(), value.clone());
            }
        }
        if joined.tips.is_empty() {
            return Err(Invalid);
        }
        Ok(joined)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    version: u8,
    pub id: String,
    pub kind: Kind,
    fields: BTreeMap<String, Register>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Invalid;

pub fn valid_id(s: &str) -> bool {
    s.len() == 32
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

pub fn tag_name(s: &str) -> String {
    s.trim().trim_start_matches('#').trim().to_lowercase()
}

impl Document {
    pub fn new(id: String, kind: Kind) -> Self {
        Self {
            version: 1,
            id,
            kind,
            fields: BTreeMap::new(),
        }
    }
    pub fn get(&self, key: &str) -> &str {
        self.fields
            .get(key)
            .and_then(|r| {
                r.tips.iter().max_by_key(|(a, v)| {
                    (
                        matches!(key, "done" | "trash") && v.as_str() == "1",
                        r.seen.get(*a),
                        *a,
                    )
                })
            })
            .map_or("", |(_, v)| v.as_str())
    }
    pub fn flag(&self, key: &str) -> bool {
        self.get(key) == "1"
    }
    pub fn tags(&self) -> Vec<String> {
        self.fields
            .keys()
            .filter(|k| self.flag(k))
            .filter_map(|k| k.strip_prefix("tag:").map(str::to_owned))
            .collect()
    }
    pub fn alternatives(&self, key: &str) -> Vec<String> {
        self.fields
            .get(key)
            .into_iter()
            .flat_map(|r| r.tips.values().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn set(&mut self, actor: &str, key: &str, value: String) -> Result<(), Invalid> {
        if !valid_id(actor) || !self.accepts(key, &value) {
            return Err(Invalid);
        }
        let field = self.fields.entry(key.into()).or_default();
        if field.tips.len() == 1 && field.tips.values().next() == Some(&value) {
            return Ok(());
        }
        field.set(actor, value)
    }
    fn accepts(&self, key: &str, v: &str) -> bool {
        let item = matches!(self.kind, Kind::Task | Kind::Note);
        match key {
            "created" => {
                self.kind == Kind::Step && v.parse::<u64>().is_ok_and(|n| n.to_string() == v)
            }
            "title" => v.len() <= 4096,
            "body" => item && v.len() <= 256 * 1024,
            "trash" => matches!(v, "0" | "1"),
            "star" => item && matches!(v, "0" | "1"),
            "done" => matches!(self.kind, Kind::Task | Kind::Step) && matches!(v, "0" | "1"),
            "parent" => self.kind != Kind::Group && (v.is_empty() || valid_id(v) && v != self.id),
            "color" => matches!(v, "iris" | "mint" | "rose" | "amber" | "blue" | "slate"),
            "due" | "day" => item && (v.is_empty() || DayDate::parse_iso(v).is_some()),
            tag if tag.starts_with("tag:") => {
                let name = &tag[4..];
                item && !name.is_empty()
                    && name.len() <= 80
                    && tag_name(name) == name
                    && !name.chars().any(|c| c.is_control() || c == ',')
                    && matches!(v, "0" | "1")
            }
            _ => false,
        }
    }
    fn validate(&self, expected: &str) -> Result<(), Invalid> {
        if self.version != 1
            || self.id != expected
            || !valid_id(expected)
            || !self.fields.contains_key("title")
            || self.fields.len() > 1024
        {
            return Err(Invalid);
        }
        for (key, r) in &self.fields {
            if r.tips.is_empty()
                || r.seen.len() > 1024
                || r.seen
                    .iter()
                    .any(|(a, &c)| !valid_id(a) || c == 0 || c >= i64::MAX as u64)
                || r.tips
                    .iter()
                    .any(|(a, v)| !r.seen.contains_key(a) || !self.accepts(key, v))
            {
                return Err(Invalid);
            }
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, Invalid> {
        self.validate(&self.id)?;
        let bytes = serde_json::to_vec(self).map_err(|_| Invalid)?;
        if bytes.len() > MAX_BYTES {
            return Err(Invalid);
        }
        Ok(bytes)
    }
    pub fn decode(expected: &str, bytes: &[u8]) -> Result<Self, Invalid> {
        if bytes.len() > MAX_BYTES {
            return Err(Invalid);
        }
        let d: Self = serde_json::from_slice(bytes).map_err(|_| Invalid)?;
        d.validate(expected)?;
        Ok(d)
    }
    pub fn join(&self, other: &Self) -> Result<Self, Invalid> {
        if self.id != other.id || self.kind != other.kind || self.version != other.version {
            return Err(Invalid);
        }
        let mut joined = self.clone();
        for (key, r) in &other.fields {
            let field = match joined.fields.get(key) {
                Some(local) => local.join(r)?,
                None => r.clone(),
            };
            joined.fields.insert(key.clone(), field);
        }
        joined.validate(&self.id)?;
        Ok(joined)
    }
}

pub type Library = BTreeMap<String, Document>;

pub fn in_scope(d: &Document, scope: &str, library: &Library, today: &str) -> bool {
    if !matches!(d.kind, Kind::Task | Kind::Note) {
        return false;
    }
    if scope == "trash" {
        return d.flag("trash");
    }
    if d.flag("trash") {
        return false;
    }
    if scope == "completed" {
        return d.flag("done");
    }
    if d.flag("done") {
        return false;
    }
    match scope {
        "all" => true,
        "inbox" => library
            .get(d.get("parent"))
            .is_none_or(|p| p.kind != Kind::List || p.flag("trash")),
        "today" => d.get("day") == today || (!d.get("due").is_empty() && d.get("due") <= today),
        "planned" => !d.get("due").is_empty(),
        "starred" => d.flag("star"),
        "notes" => d.kind == Kind::Note,
        tag if tag.starts_with("tag:") => d.flag(tag),
        list => d.get("parent") == list,
    }
}
