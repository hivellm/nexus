//! Streaming reader for the pipe-separated LDBC CSVs, plus the value coercion
//! the graph side needs.
//!
//! Streaming matters: `comment_0_0.csv` alone is 17 MiB at SF0.1 and 1 GiB at
//! SF10, and the loader makes two passes over every node file (nodes, then the
//! merge-foreign edges), so nothing may be held in memory between passes.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::fs::File;
use std::path::Path;

use crate::schema::Coerce;

/// An open LDBC CSV with its header row resolved.
pub struct CsvSource {
    reader: csv::Reader<File>,
    headers: Vec<String>,
    path: String,
}

impl CsvSource {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(b'|')
            // LDBC CSVs are not RFC4180-quoted: `content` fields contain bare
            // double quotes, which the default quoting rules would swallow
            // together with the rest of the row.
            .quoting(false)
            .flexible(false)
            .from_reader(file);
        let headers = reader
            .headers()
            .with_context(|| format!("reading the header of {}", path.display()))?
            .iter()
            .map(str::to_string)
            .collect();
        Ok(Self {
            reader,
            headers,
            path: path.display().to_string(),
        })
    }

    /// Index of the first column with this name.
    pub fn column(&self, name: &str) -> Result<usize> {
        self.headers
            .iter()
            .position(|h| h == name)
            .with_context(|| {
                format!(
                    "{} has no `{name}` column (header: {:?})",
                    self.path, self.headers
                )
            })
    }

    /// Index of the `nth` (0-based) column with this name.
    ///
    /// `person_knows_person` repeats `Person.id` for both endpoints, so the
    /// two sides can only be told apart positionally.
    pub fn nth_column(&self, name: &str, nth: usize) -> Result<usize> {
        self.headers
            .iter()
            .enumerate()
            .filter(|(_, h)| *h == name)
            .map(|(i, _)| i)
            .nth(nth)
            .with_context(|| {
                format!(
                    "{} has no {} occurrence of column `{name}` (header: {:?})",
                    self.path,
                    nth + 1,
                    self.headers
                )
            })
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    /// Iterate the data rows (the header is already consumed).
    pub fn rows(&mut self) -> impl Iterator<Item = Result<csv::StringRecord>> + '_ {
        let path = self.path.clone();
        self.reader
            .records()
            .map(move |r| r.with_context(|| format!("reading a row of {path}")))
    }
}

/// Convert one CSV field into the JSON value stored on the node/relationship.
///
/// An EMPTY field yields `None` — the property is omitted entirely rather than
/// stored as null. LDBC leaves fields blank to mean "absent" (`post.content`
/// for an image post, `post.imageFile` for a text post, `comment.replyOfPost`
/// on a reply to a comment), and the reference queries reach for
/// `coalesce(...)`, which needs a missing property, not a null-valued one.
pub fn coerce(field: &str, how: Coerce) -> Result<Option<Value>> {
    if field.is_empty() {
        return Ok(None);
    }
    Ok(Some(match how {
        Coerce::Int => {
            let parsed: i64 = field
                .parse()
                .with_context(|| format!("`{field}` is not an integer"))?;
            Value::from(parsed)
        }
        Coerce::Str => Value::String(field.to_string()),
        Coerce::StrList => Value::Array(
            field
                .split(';')
                .filter(|part| !part.is_empty())
                .map(|part| Value::String(part.to_string()))
                .collect(),
        ),
    }))
}

/// Parse a foreign-key / endpoint column into an LDBC id. Empty means "no
/// edge" (continents have no `isPartOf`), which is not an error.
pub fn parse_id(field: &str, what: &str) -> Result<Option<i64>> {
    if field.is_empty() {
        return Ok(None);
    }
    match field.parse::<i64>() {
        Ok(id) => Ok(Some(id)),
        Err(e) => bail!("{what}: `{field}` is not a valid LDBC id ({e})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_csv(name: &str, contents: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("ldbc-loader-test-{name}.csv"));
        let mut file = File::create(&path).expect("create temp csv");
        file.write_all(contents.as_bytes()).expect("write temp csv");
        path
    }

    #[test]
    fn empty_fields_omit_the_property() {
        // A blank `content` must leave the property absent so `coalesce` in the
        // reference queries picks the other one, not a stored null.
        assert_eq!(coerce("", Coerce::Str).unwrap(), None);
        assert_eq!(coerce("", Coerce::Int).unwrap(), None);
        assert_eq!(coerce("", Coerce::StrList).unwrap(), None);
    }

    #[test]
    fn dates_stay_epoch_milliseconds() {
        // `LongDateFormatter` output, and what the substitution parameters
        // compare against — no ISO-8601 round trip.
        assert_eq!(
            coerce("1266161530447", Coerce::Int).unwrap(),
            Some(Value::from(1_266_161_530_447i64))
        );
    }

    #[test]
    fn ids_beyond_32_bits_survive() {
        // Post/Comment ids at SF1 are well past u32.
        assert_eq!(
            coerce("618475290624", Coerce::Int).unwrap(),
            Some(Value::from(618_475_290_624i64))
        );
    }

    #[test]
    fn multi_values_split_on_semicolons() {
        assert_eq!(
            coerce("si;en", Coerce::StrList).unwrap(),
            Some(Value::Array(vec![
                Value::String("si".into()),
                Value::String("en".into())
            ]))
        );
        // A single value is still a list — the property type must not change
        // shape with the row.
        assert_eq!(
            coerce("en", Coerce::StrList).unwrap(),
            Some(Value::Array(vec![Value::String("en".into())]))
        );
    }

    #[test]
    fn a_non_numeric_int_column_is_an_error_not_a_zero() {
        assert!(coerce("not-a-number", Coerce::Int).is_err());
        assert!(parse_id("oops", "test column").is_err());
    }

    #[test]
    fn an_empty_foreign_key_means_no_edge() {
        assert_eq!(parse_id("", "isPartOf").unwrap(), None);
        assert_eq!(parse_id("1454", "isPartOf").unwrap(), Some(1454));
    }

    #[test]
    fn pipe_separated_rows_parse_with_bare_quotes_intact() {
        // LDBC `content` fields contain unescaped double quotes; with CSV
        // quoting enabled the reader would treat one as an opening quote and
        // merge the rest of the file into a single field.
        let path = temp_csv("quotes", "id|content|length\n42|he said \"hi\" loudly|19\n");
        let mut source = CsvSource::open(&path).expect("open");
        let content = source.column("content").expect("content column");
        let rows: Vec<_> = source.rows().map(|r| r.expect("row")).collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(&rows[0][content], "he said \"hi\" loudly");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn repeated_column_names_are_addressable_by_position() {
        // `person_knows_person` is `Person.id|Person.id|creationDate`.
        let path = temp_csv(
            "knows",
            "Person.id|Person.id|creationDate\n1|2|1600000000000\n",
        );
        let source = CsvSource::open(&path).expect("open");
        assert_eq!(source.nth_column("Person.id", 0).unwrap(), 0);
        assert_eq!(source.nth_column("Person.id", 1).unwrap(), 1);
        assert!(source.nth_column("Person.id", 2).is_err());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_missing_column_is_reported_with_the_header() {
        let path = temp_csv("missing", "id|name\n1|x\n");
        let source = CsvSource::open(&path).expect("open");
        let error = source.column("nope").unwrap_err().to_string();
        assert!(
            error.contains("nope"),
            "error must name the column: {error}"
        );
        std::fs::remove_file(&path).ok();
    }
}
