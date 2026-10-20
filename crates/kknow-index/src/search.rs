//! Full-text search over notes.
//!
//! The index lives in memory and is built when the vault opens. The files are
//! the source of truth, and the index is derived from them and can always be
//! rebuilt. Saving it to disk is a separate task.

use kknow_core::NoteId;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, BoostQuery, Occur, Query, QueryParser, RegexQuery};
use tantivy::schema::{
    Field, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions, Value,
};
use tantivy::snippet::SnippetGenerator;
use tantivy::tokenizer::{LowerCaser, RemoveLongFilter, SimpleTokenizer, Stemmer, TextAnalyzer};
use tantivy::{Index, IndexReader, IndexWriter, TantivyDocument, Term, doc};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("ошибка поискового индекса: {0}")]
    Tantivy(#[from] tantivy::TantivyError),

    #[error("не удалось разобрать запрос: {0}")]
    Query(#[from] tantivy::query::QueryParserError),
}

/// Name of the analyzer that all text fields share.
const ANALYZER: &str = "kknow";

/// Number of characters in a result snippet.
const SNIPPET_CHARS: usize = 180;

#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub id: NoteId,
    pub title: String,
    pub score: f32,
    /// Text fragment with `<mark>` around matches.
    pub snippet: String,
}

struct Fields {
    id: Field,
    title: Field,
    body: Field,
    tag: Field,
    path: Field,
}

pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    writer: IndexWriter,
    fields: Fields,
}

impl SearchIndex {
    pub fn new() -> Result<Self> {
        let mut schema = Schema::builder();

        // `id` is not tokenized: it is the key to delete a document.
        let id = schema.add_text_field("id", STRING | STORED);
        let title = schema.add_text_field("title", analyzed() | STORED);
        let body = schema.add_text_field("body", analyzed() | STORED);
        let tag = schema.add_text_field("tag", analyzed());
        let path = schema.add_text_field("path", analyzed());

        let index = Index::create_in_ram(schema.build());
        index.tokenizers().register(ANALYZER, analyzer());

        let writer = index.writer(50_000_000)?;
        let reader = index.reader()?;

        Ok(SearchIndex {
            index,
            reader,
            writer,
            fields: Fields {
                id,
                title,
                body,
                tag,
                path,
            },
        })
    }

    /// Adds or replaces the note document.
    ///
    /// Changes become visible only after [`SearchIndex::commit`].
    pub fn upsert(&mut self, id: &NoteId, title: &str, body: &str, tags: &[String]) -> Result<()> {
        self.remove(id)?;

        self.writer.add_document(doc!(
            self.fields.id => id.as_str(),
            self.fields.title => title,
            self.fields.body => body,
            self.fields.tag => tags.join(" "),
            // The path is searched as text, so "projects rust" finds a note by folder.
            self.fields.path => id.as_str().replace('/', " "),
        ))?;
        Ok(())
    }

    pub fn remove(&mut self, id: &NoteId) -> Result<()> {
        self.writer
            .delete_term(Term::from_field_text(self.fields.id, id.as_str()));
        Ok(())
    }

    /// Makes the pending changes visible.
    pub fn commit(&mut self) -> Result<()> {
        self.writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Supports tantivy syntax: `tag:rust`, `title:kknow`, `-word`, quotes for
    /// a phrase. All plain words are required.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let parsed = self.parser().parse_query(query)?;

        // A prefix match is always added to the exact one: while the user
        // types «синхр», they must already see «синхронизацию». Exact matches
        // weigh more, so they get a boost.
        let query: Box<dyn Query> = match self.prefix_query(query)? {
            Some(prefix) => Box::new(BooleanQuery::new(vec![
                (Occur::Should, Box::new(BoostQuery::new(parsed, 3.0))),
                (Occur::Should, prefix),
            ])),
            None => parsed,
        };

        self.run(query, limit)
    }

    fn parser(&self) -> QueryParser {
        let mut parser = QueryParser::for_index(&self.index, self.searchable().to_vec());
        parser.set_conjunction_by_default();
        parser.set_field_boost(self.fields.title, 4.0);
        parser.set_field_boost(self.fields.tag, 2.0);
        parser.set_field_boost(self.fields.path, 1.5);
        parser
    }

    fn searchable(&self) -> [Field; 4] {
        [
            self.fields.title,
            self.fields.body,
            self.fields.tag,
            self.fields.path,
        ]
    }

    /// A query where the last word is treated as the start of a word.
    ///
    /// `*` in tantivy syntax gives no prefix search: the tokenizer drops the
    /// star. So the prefix is searched with a regex over the term dictionary.
    fn prefix_query(&self, query: &str) -> Result<Option<Box<dyn Query>>> {
        if !query
            .chars()
            .all(|ch| ch.is_alphanumeric() || ch.is_whitespace() || ch == '_')
        {
            return Ok(None);
        }

        let words: Vec<&str> = query.split_whitespace().collect();
        let Some((last, head)) = words.split_last() else {
            return Ok(None);
        };

        let pattern = format!("{}.*", last.to_lowercase());
        let mut prefix_clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        for field in self.searchable() {
            let regex = RegexQuery::from_pattern(&pattern, field)?;
            prefix_clauses.push((Occur::Should, Box::new(regex)));
        }

        let mut clauses: Vec<(Occur, Box<dyn Query>)> =
            vec![(Occur::Must, Box::new(BooleanQuery::new(prefix_clauses)))];
        if !head.is_empty() {
            clauses.push((Occur::Must, self.parser().parse_query(&head.join(" "))?));
        }

        Ok(Some(Box::new(BooleanQuery::new(clauses))))
    }

    fn run(&self, query: Box<dyn Query>, limit: usize) -> Result<Vec<SearchHit>> {
        let searcher = self.reader.searcher();
        let found = searcher.search(&query, &TopDocs::with_limit(limit))?;

        let mut snippets = SnippetGenerator::create(&searcher, &*query, self.fields.body)?;
        snippets.set_max_num_chars(SNIPPET_CHARS);

        let mut hits = Vec::with_capacity(found.len());
        for (score, address) in found {
            let document: TantivyDocument = searcher.doc(address)?;

            let Some(id) = text(&document, self.fields.id)
                .as_deref()
                .and_then(NoteId::parse)
            else {
                continue;
            };
            let title = text(&document, self.fields.title).unwrap_or_else(|| id.name().to_string());

            let snippet = snippets.snippet_from_doc(&document);
            let snippet = if snippet.fragment().is_empty() {
                text(&document, self.fields.body)
                    .map(|body| shorten(&body, SNIPPET_CHARS))
                    .unwrap_or_default()
            } else {
                snippet.to_html()
            };

            hits.push(SearchHit {
                id,
                title,
                score,
                snippet,
            });
        }

        Ok(hits)
    }
}

fn analyzed() -> TextOptions {
    TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer(ANALYZER)
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    )
}

/// The Russian stemmer brings «заметки» and «заметка» to one form. It almost
/// does not touch Latin text, so one analyzer is enough for both languages.
fn analyzer() -> TextAnalyzer {
    TextAnalyzer::builder(SimpleTokenizer::default())
        .filter(RemoveLongFilter::limit(40))
        .filter(LowerCaser)
        .filter(Stemmer::new(tantivy::tokenizer::Language::Russian))
        .build()
}

fn text(document: &TantivyDocument, field: Field) -> Option<String> {
    document
        .get_first(field)
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

fn shorten(text: &str, limit: usize) -> String {
    let mut result: String = text.chars().take(limit).collect();
    if text.chars().count() > limit {
        result.push('…');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(notes: &[(&str, &str, &str, &[&str])]) -> SearchIndex {
        let mut index = SearchIndex::new().unwrap();
        for (id, title, body, tags) in notes {
            let tags: Vec<String> = tags.iter().map(|tag| tag.to_string()).collect();
            index
                .upsert(&NoteId::parse(id).unwrap(), title, body, &tags)
                .unwrap();
        }
        index.commit().unwrap();
        index
    }

    fn ids(hits: &[SearchHit]) -> Vec<&str> {
        hits.iter().map(|hit| hit.id.as_str()).collect()
    }

    fn sample() -> SearchIndex {
        index(&[
            (
                "Rust",
                "Rust",
                "Системный язык программирования без сборщика мусора",
                &["язык"],
            ),
            (
                "Projects/kknow",
                "kknow",
                "Проект: заметки в markdown, поиск и синхронизация",
                &["rust", "проекты/kknow"],
            ),
            (
                "Daily/2026-07-26",
                "2026-07-26",
                "Сегодня писал поисковый индекс для заметок",
                &["дневник"],
            ),
        ])
    }

    #[test]
    fn finds_by_word_from_the_body() {
        let hits = sample().search("синхронизация", 10).unwrap();

        assert_eq!(ids(&hits), ["Projects/kknow"]);
    }

    #[test]
    fn word_forms_match_thanks_to_stemming() {
        let index = sample();

        // The text has «синхронизация». Search with other grammatical cases.
        assert_eq!(
            ids(&index.search("синхронизации", 10).unwrap()),
            ["Projects/kknow"]
        );
        assert_eq!(
            ids(&index.search("синхронизацией", 10).unwrap()),
            ["Projects/kknow"]
        );

        // The stemmer cuts endings but does not restore a dropped vowel:
        // «заметки» and «заметок» stay different words. Russian lemmatization
        // is a separate task. For now this is accepted.
        assert_eq!(
            ids(&index.search("заметками", 10).unwrap()),
            ["Projects/kknow"]
        );
    }

    #[test]
    fn all_words_must_match() {
        let index = sample();

        assert_eq!(
            ids(&index.search("поисковый индекс", 10).unwrap()),
            ["Daily/2026-07-26"]
        );
        assert!(
            index
                .search("поисковый несуществующее", 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn title_outranks_body() {
        let hits = sample().search("kknow", 10).unwrap();

        assert_eq!(hits[0].id.as_str(), "Projects/kknow");
    }

    #[test]
    fn searches_by_tag_with_explicit_field() {
        let hits = sample().search("tag:rust", 10).unwrap();

        assert_eq!(ids(&hits), ["Projects/kknow"]);
    }

    #[test]
    fn searches_by_folder_name() {
        let hits = sample().search("daily", 10).unwrap();

        assert_eq!(ids(&hits), ["Daily/2026-07-26"]);
    }

    #[test]
    fn prefix_search_works_while_typing() {
        let index = sample();

        // «синхр» is an incomplete word. Plain search does not find it.
        assert_eq!(ids(&index.search("синхр", 10).unwrap()), ["Projects/kknow"]);
    }

    #[test]
    fn snippet_highlights_the_match() {
        let hits = sample().search("мусора", 10).unwrap();

        assert!(
            hits[0].snippet.contains("<b>мусора</b>"),
            "{}",
            hits[0].snippet
        );
    }

    #[test]
    fn upsert_replaces_the_previous_version() {
        let mut index = sample();
        let id = NoteId::parse("Rust").unwrap();

        index
            .upsert(&id, "Rust", "Теперь тут совсем другой текст", &[])
            .unwrap();
        index.commit().unwrap();

        assert!(index.search("мусора", 10).unwrap().is_empty());
        assert_eq!(ids(&index.search("другой", 10).unwrap()), ["Rust"]);
    }

    #[test]
    fn removed_notes_disappear_from_results() {
        let mut index = sample();

        index
            .remove(&NoteId::parse("Projects/kknow").unwrap())
            .unwrap();
        index.commit().unwrap();

        assert!(index.search("синхронизация", 10).unwrap().is_empty());
    }

    #[test]
    fn empty_query_returns_nothing() {
        assert!(sample().search("   ", 10).unwrap().is_empty());
    }

    #[test]
    fn broken_query_is_an_error_not_a_panic() {
        assert!(sample().search("title:(", 10).is_err());
    }
}
