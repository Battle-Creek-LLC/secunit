//! In-memory search index for the ⌘K palette.
//!
//! Built on Tantivy with a `RamDirectory` so the index never touches
//! disk. Schema fields:
//!
//! | field    | type   | indexed | stored | tokenizer |
//! |----------|--------|---------|--------|-----------|
//! | kind     | text   | yes     | yes    | raw       |
//! | id       | text   | yes     | yes    | simple    |
//! | title    | text   | yes     | yes    | en_stem   |
//! | tags     | text   | yes     | yes    | raw       |
//! | body     | text   | yes     | yes    | en_stem   |
//! | path     | text   | no      | yes    | —         |
//! | mtime    | i64    | yes     | yes    | —         |
//! | status   | text   | yes     | yes    | raw       |
//!
//! Title and tags are boosted at query-time via per-field weights.

use std::path::Path;

use chrono::{DateTime, Utc};
use secunit_core::model::LoadedRegistry;
use serde::Serialize;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser};
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value, FAST, INDEXED, STORED,
    STRING, TEXT,
};
use tantivy::{doc, Index, IndexWriter, TantivyDocument};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SearchHit {
    pub kind: String,
    pub id: String,
    pub title: String,
    pub path: String,
    pub status: Option<String>,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexStatus {
    pub ready: bool,
    pub doc_count: usize,
    pub last_updated: DateTime<Utc>,
}

pub struct SearchIndex {
    fields: Fields,
    index: Index,
    doc_count: usize,
    last_updated: DateTime<Utc>,
}

#[derive(Clone)]
struct Fields {
    kind: Field,
    id: Field,
    title: Field,
    tags: Field,
    body: Field,
    path: Field,
    mtime: Field,
    status: Field,
}

fn schema() -> (Schema, Fields) {
    let mut b = Schema::builder();

    let raw = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("raw")
                .set_index_option(IndexRecordOption::Basic),
        )
        .set_stored();
    let kind = b.add_text_field("kind", raw.clone() | STORED);
    let id = b.add_text_field("id", TEXT | STORED);
    let title = b.add_text_field("title", TEXT | STORED);
    let tags = b.add_text_field("tags", raw.clone() | STORED);
    let body = b.add_text_field("body", TEXT | STORED);
    let path = b.add_text_field("path", STRING | STORED);
    let mtime = b.add_i64_field("mtime", INDEXED | STORED | FAST);
    let status = b.add_text_field("status", raw | STORED);

    let schema = b.build();
    (
        schema,
        Fields {
            kind,
            id,
            title,
            tags,
            body,
            path,
            mtime,
            status,
        },
    )
}

impl SearchIndex {
    /// Build an empty index. Use `rebuild` to populate.
    pub fn new() -> tantivy::Result<Self> {
        let (schema, fields) = schema();
        let index = Index::create_in_ram(schema);
        Ok(Self {
            fields,
            index,
            doc_count: 0,
            last_updated: Utc::now(),
        })
    }

    pub fn status(&self) -> IndexStatus {
        IndexStatus {
            ready: true,
            doc_count: self.doc_count,
            last_updated: self.last_updated,
        }
    }

    /// Wipe and re-build from the loaded registry + on-disk evidence.
    pub fn rebuild(&mut self, reg: &LoadedRegistry, root: &Path) -> tantivy::Result<()> {
        let mut writer: IndexWriter = self.index.writer(50_000_000)?;
        writer.delete_all_documents()?;
        let mut count = 0usize;

        for (id, ctrl) in &reg.controls {
            writer.add_document(doc!(
                self.fields.kind => "control",
                self.fields.id => id.as_str(),
                self.fields.title => ctrl.title.as_str(),
                self.fields.tags => ctrl.nist.join(" "),
                self.fields.body => format!(
                    "{} {} {} {}",
                    ctrl.title, ctrl.policy, ctrl.skill, ctrl.owner
                ),
                self.fields.path => format!("controls/{id}.yaml"),
                self.fields.mtime => 0i64,
                self.fields.status => "",
            ))?;
            count += 1;
        }

        for (kind, entries) in &reg.inventory.kinds {
            for entry in entries {
                writer.add_document(doc!(
                    self.fields.kind => "inventory",
                    self.fields.id => entry.name.as_str(),
                    self.fields.title => entry.name.as_str(),
                    self.fields.tags => format!("{} {}", kind, entry.tags.join(" ")),
                    self.fields.body => entry.aliases.join(" "),
                    self.fields.path => "inventory.yaml",
                    self.fields.mtime => 0i64,
                    self.fields.status => if entry.retired_on.is_some() { "retired" } else { "active" },
                ))?;
                count += 1;
            }
        }

        let evidence = root.join("evidence");
        if evidence.exists() {
            for entry in walkdir::WalkDir::new(&evidence).into_iter().flatten() {
                if !entry.file_type().is_file() {
                    continue;
                }
                let p = entry.path();
                let name = entry.file_name().to_string_lossy();
                let path_str = p.display().to_string();
                let parts: Vec<&str> = path_str
                    .strip_prefix(&format!("{}/", evidence.display()))
                    .unwrap_or(&path_str)
                    .split('/')
                    .collect();

                let mtime = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(|t| {
                        let dur = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                        dur.as_secs() as i64
                    })
                    .unwrap_or(0);

                if name == "manifest.json" || name == "prepare.json" {
                    if let (Some(c), Some(r)) = (parts.get(2), parts.get(3)) {
                        let status = match name.as_ref() {
                            "manifest.json" => "sealed",
                            _ => "pending",
                        };
                        writer.add_document(doc!(
                            self.fields.kind => "run",
                            self.fields.id => format!("{c}/{r}"),
                            self.fields.title => format!("{c} {r}"),
                            self.fields.tags => "",
                            self.fields.body => "",
                            self.fields.path => path_str.clone(),
                            self.fields.mtime => mtime,
                            self.fields.status => status,
                        ))?;
                        count += 1;
                    }
                }

                if name == "findings.md" {
                    let body = std::fs::read_to_string(p).unwrap_or_default();
                    let title = body
                        .lines()
                        .find(|l| l.starts_with("# "))
                        .map(|l| l.trim_start_matches('#').trim().to_string())
                        .unwrap_or_else(|| {
                            parts.get(2..4).map(|s| s.join("/")).unwrap_or_default()
                        });
                    let id = parts
                        .get(2..4)
                        .map(|s| s.join("/"))
                        .unwrap_or_else(|| "?".into());
                    writer.add_document(doc!(
                        self.fields.kind => "finding",
                        self.fields.id => id.as_str(),
                        self.fields.title => title.as_str(),
                        self.fields.tags => "",
                        self.fields.body => body.as_str(),
                        self.fields.path => path_str.clone(),
                        self.fields.mtime => mtime,
                        self.fields.status => "",
                    ))?;
                    count += 1;
                }

                if !["manifest.json", "prepare.json", "findings.md"].contains(&name.as_ref()) {
                    writer.add_document(doc!(
                        self.fields.kind => "artifact",
                        self.fields.id => path_str.clone(),
                        self.fields.title => name.into_owned(),
                        self.fields.tags => "",
                        self.fields.body => "",
                        self.fields.path => path_str.clone(),
                        self.fields.mtime => mtime,
                        self.fields.status => "",
                    ))?;
                    count += 1;
                }
            }
        }

        writer.commit()?;
        self.doc_count = count;
        self.last_updated = Utc::now();
        Ok(())
    }

    /// BM25 search with field boosts. `kinds` filters the result set if
    /// non-empty.
    pub fn search(
        &self,
        query: &str,
        limit: usize,
        kinds: &[String],
    ) -> tantivy::Result<Vec<SearchHit>> {
        let reader = self.index.reader()?;
        let searcher = reader.searcher();

        let mut parser = QueryParser::for_index(
            &self.index,
            vec![
                self.fields.title,
                self.fields.tags,
                self.fields.body,
                self.fields.id,
            ],
        );
        parser.set_field_boost(self.fields.title, 4.0);
        parser.set_field_boost(self.fields.tags, 3.0);
        parser.set_field_boost(self.fields.id, 2.0);
        parser.set_conjunction_by_default();

        let user_query = parser
            .parse_query(query)
            .map_err(|e| tantivy::TantivyError::InvalidArgument(format!("parse: {e}")))?;

        let q: Box<dyn Query> = if kinds.is_empty() {
            user_query
        } else {
            // AND the user query with a kind filter (OR over allowed kinds).
            let kind_or: Vec<(Occur, Box<dyn Query>)> = kinds
                .iter()
                .map(|k| {
                    let q = parser
                        .parse_query(&format!("kind:{k}"))
                        .expect("kind filter parse");
                    (Occur::Should, q)
                })
                .collect();
            let kind_query = Box::new(BooleanQuery::new(kind_or));
            Box::new(BooleanQuery::new(vec![
                (Occur::Must, user_query),
                (Occur::Must, kind_query),
            ]))
        };

        let top = searcher.search(&q, &TopDocs::with_limit(limit).order_by_score())?;
        let mut hits = Vec::with_capacity(top.len());
        for (score, addr) in top {
            let doc = searcher.doc::<TantivyDocument>(addr)?;
            hits.push(SearchHit {
                kind: doc
                    .get_first(self.fields.kind)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                id: doc
                    .get_first(self.fields.id)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                title: doc
                    .get_first(self.fields.title)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                path: doc
                    .get_first(self.fields.path)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                status: doc
                    .get_first(self.fields.status)
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty()),
                score,
            });
        }
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secunit_core::registry::loader;
    use std::path::PathBuf;

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("testdata/orgs/multi-system")
    }

    #[test]
    fn empty_index_status() {
        let idx = SearchIndex::new().unwrap();
        let s = idx.status();
        assert!(s.ready);
        assert_eq!(s.doc_count, 0);
    }

    #[test]
    fn finds_a_control_by_id_substring() {
        let root = fixture_root();
        if !root.exists() {
            return;
        }
        let (reg, _) = loader::load(&root);
        let mut idx = SearchIndex::new().unwrap();
        idx.rebuild(&reg, &root).unwrap();
        let hits = idx.search("audit", 10, &[]).unwrap();
        assert!(!hits.is_empty(), "expected hits for 'audit'");
        assert!(
            hits.iter().any(|h| h.kind == "control"),
            "expected a control kind hit"
        );
    }

    #[test]
    fn kind_filter_narrows_results() {
        let root = fixture_root();
        if !root.exists() {
            return;
        }
        let (reg, _) = loader::load(&root);
        let mut idx = SearchIndex::new().unwrap();
        idx.rebuild(&reg, &root).unwrap();
        let hits = idx.search("audit", 10, &["control".into()]).unwrap();
        assert!(hits.iter().all(|h| h.kind == "control"));
    }

    #[test]
    fn rebuild_reflects_a_new_control() {
        let root = fixture_root();
        if !root.exists() {
            return;
        }
        let (reg, _) = loader::load(&root);
        let mut idx = SearchIndex::new().unwrap();
        idx.rebuild(&reg, &root).unwrap();
        let baseline = idx.status().doc_count;

        // Build a fake registry with one extra control.
        let mut reg2 = reg.clone();
        reg2.controls.insert(
            "synthetic-zzz".into(),
            secunit_core::model::Control {
                id: "synthetic-zzz".into(),
                title: "Synthetic zzz".into(),
                policy: "security/x.md".into(),
                nist: vec![],
                owner: "x@example".into(),
                cadence: secunit_core::model::Cadence::Weekly,
                weekday: None,
                due: None,
                due_by: None,
                skill: "skill".into(),
                skill_args: None,
                scope: None,
                evidence_required: vec![],
                remediation_thresholds: Default::default(),
                outputs: None,
                references: vec![],
            },
        );
        idx.rebuild(&reg2, &root).unwrap();
        assert_eq!(idx.status().doc_count, baseline + 1);

        let hits = idx.search("synthetic", 5, &[]).unwrap();
        assert!(hits.iter().any(|h| h.id == "synthetic-zzz"));
    }
}
