/*
 * Local Tantivy full-text index for Horus (owned search corpus).
 */

use anyhow::{Context, Result};
use std::path::Path;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Schema, STORED, TEXT, Value};
use tantivy::{doc, Index, IndexWriter, ReloadPolicy, TantivyDocument};

use crate::{NetworkType, ResultBadge, SearchResult};

pub struct TantivyIndex {
    index: Index,
    schema: Schema,
}

impl TantivyIndex {
    pub fn open_or_create(path: &Path) -> Result<Self> {
        std::fs::create_dir_all(path)?;
        let mut schema_builder = Schema::builder();
        schema_builder.add_text_field("title", TEXT | STORED);
        schema_builder.add_text_field("url", TEXT | STORED);
        schema_builder.add_text_field("body", TEXT | STORED);
        schema_builder.add_text_field("source", TEXT | STORED);
        let schema = schema_builder.build();

        let index = if path.join("meta.json").exists() {
            Index::open_in_dir(path).context("open tantivy")?
        } else {
            Index::create_in_dir(path, schema.clone()).context("create tantivy")?
        };

        Ok(Self { index, schema })
    }

    pub fn len(&self) -> Result<usize> {
        let reader = self.index.reader()?;
        Ok(reader.searcher().num_docs() as usize)
    }

    pub fn add_document(&self, result: &SearchResult) -> Result<()> {
        let title = self.schema.get_field("title").unwrap();
        let url = self.schema.get_field("url").unwrap();
        let body = self.schema.get_field("body").unwrap();
        let source = self.schema.get_field("source").unwrap();
        let mut writer: IndexWriter = self.index.writer(50_000_000)?;
        writer.add_document(doc!(
            title => result.title.as_str(),
            url => result.url.as_str(),
            body => result.description.as_str(),
            source => format!("{:?}", result.source),
        ))?;
        writer.commit()?;
        Ok(())
    }

    pub fn bulk_index(&self, docs: &[SearchResult]) -> Result<()> {
        let title = self.schema.get_field("title").unwrap();
        let url = self.schema.get_field("url").unwrap();
        let body = self.schema.get_field("body").unwrap();
        let source = self.schema.get_field("source").unwrap();
        let mut writer: IndexWriter = self.index.writer(50_000_000)?;
        for result in docs {
            writer.add_document(doc!(
                title => result.title.as_str(),
                url => result.url.as_str(),
                body => result.description.as_str(),
                source => format!("{:?}", result.source),
            ))?;
        }
        writer.commit()?;
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        let searcher = reader.searcher();
        let title = self.schema.get_field("title").unwrap();
        let body = self.schema.get_field("body").unwrap();
        let url_f = self.schema.get_field("url").unwrap();
        let parser = QueryParser::for_index(&self.index, vec![title, body]);
        let q = match parser.parse_query(query) {
            Ok(q) => q,
            Err(_) => return Ok(vec![]),
        };
        let top = searcher.search(&q, &TopDocs::with_limit(limit))?;
        let mut out = Vec::new();
        for (_score, addr) in top {
            let retrieved: TantivyDocument = searcher.doc(addr)?;
            let get = |field| {
                retrieved
                    .get_first(field)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            let link = get(url_f);
            let is_onion = link.contains(".onion");
            out.push(SearchResult {
                title: get(title),
                url: link.clone(),
                description: get(body),
                source: if is_onion {
                    NetworkType::Tor
                } else {
                    NetworkType::SurfaceWeb
                },
                verified: true,
                badge: if is_onion {
                    ResultBadge::Onion
                } else {
                    ResultBadge::Local
                },
            });
        }
        Ok(out)
    }
}
