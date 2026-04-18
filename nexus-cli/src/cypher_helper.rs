use rustyline::completion::{Completer, Pair};
use rustyline::highlight::{Highlighter, MatchingBracketHighlighter};
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::validate::{self, ValidationContext, ValidationResult, Validator};
use rustyline::{Context, Helper};
use rustyline_derive::{Completer, Helper, Hinter, Validator};

/// Cypher keywords for tab completion
const CYPHER_KEYWORDS: &[&str] = &[
    // Core clauses
    "MATCH",
    "WHERE",
    "RETURN",
    "WITH",
    "CREATE",
    "MERGE",
    "DELETE",
    "DETACH",
    "REMOVE",
    "SET",
    "UNWIND",
    "UNION",
    "OPTIONAL",
    "CALL",
    "YIELD",
    // Read clauses
    "SKIP",
    "LIMIT",
    "ORDER",
    "BY",
    "ASC",
    "DESC",
    // Functions
    "count",
    "sum",
    "avg",
    "min",
    "max",
    "collect",
    "distinct",
    "labels",
    "type",
    "properties",
    "keys",
    "nodes",
    "relationships",
    "exists",
    "size",
    "length",
    "coalesce",
    "head",
    "last",
    "tail",
    "range",
    "reverse",
    "reduce",
    "toString",
    "toInteger",
    "toFloat",
    "toBoolean",
    // String functions
    "substring",
    "replace",
    "split",
    "toLower",
    "toUpper",
    "trim",
    "ltrim",
    "rtrim",
    // Math functions
    "abs",
    "ceil",
    "floor",
    "round",
    "sign",
    "rand",
    "sqrt",
    "exp",
    "log",
    "log10",
    // Temporal functions
    "date",
    "datetime",
    "time",
    "duration",
    "timestamp",
    // Spatial functions
    "point",
    "distance",
    // Aggregations
    "percentileDisc",
    "percentileCont",
    "stDev",
    "stDevP",
    // Predicates
    "all",
    "any",
    "none",
    "single",
    // Path functions
    "shortestPath",
    "allShortestPaths",
    // Operators
    "AND",
    "OR",
    "NOT",
    "XOR",
    "IN",
    "IS",
    "NULL",
    "AS",
    "ON",
    "STARTS",
    "ENDS",
    "CONTAINS",
    // Database commands
    "SHOW",
    "DATABASES",
    "USE",
    "CREATE DATABASE",
    "DROP DATABASE",
    "ALTER DATABASE",
    "ACCESS",
    "READ",
    "WRITE",
    "ONLY",
    "OPTION",
    // Index/Constraint commands
    "INDEX",
    "CONSTRAINT",
    "UNIQUE",
    "KEY",
    "FOR",
    "REQUIRE",
    // Transaction commands
    "BEGIN",
    "COMMIT",
    "ROLLBACK",
];

/// CLI-specific commands for the REPL
const CLI_COMMANDS: &[&str] = &[
    ":quit", ":exit", ":q", ":clear", ":history", ":h", ":help", ":?",
];

#[derive(Helper, Completer, Hinter, Validator)]
pub struct CypherHelper {
    #[rustyline(Completer)]
    completer: CypherCompleter,
    highlighter: MatchingBracketHighlighter,
    #[rustyline(Validator)]
    validator: CypherValidator,
    #[rustyline(Hinter)]
    hinter: HistoryHinter,
    colored_prompt: String,
}

impl CypherHelper {
    pub fn new() -> Self {
        Self {
            completer: CypherCompleter::new(),
            highlighter: MatchingBracketHighlighter::new(),
            validator: CypherValidator,
            hinter: HistoryHinter::new(),
            colored_prompt: "".to_owned(),
        }
    }

    pub fn set_colored_prompt(&mut self, prompt: String) {
        self.colored_prompt = prompt;
    }
}

impl Highlighter for CypherHelper {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        default: bool,
    ) -> std::borrow::Cow<'b, str> {
        if default {
            std::borrow::Cow::Borrowed(&self.colored_prompt)
        } else {
            std::borrow::Cow::Borrowed(prompt)
        }
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> std::borrow::Cow<'h, str> {
        use colored::Colorize;
        std::borrow::Cow::Owned(hint.dimmed().to_string())
    }

    fn highlight<'l>(&self, line: &'l str, pos: usize) -> std::borrow::Cow<'l, str> {
        self.highlighter.highlight(line, pos)
    }

    fn highlight_char(&self, line: &str, pos: usize, forced: bool) -> bool {
        self.highlighter.highlight_char(line, pos, forced)
    }
}

pub struct CypherCompleter {
    keywords: Vec<String>,
}

impl CypherCompleter {
    pub fn new() -> Self {
        let mut keywords = Vec::new();

        // Add Cypher keywords (uppercase and lowercase)
        for keyword in CYPHER_KEYWORDS {
            keywords.push(keyword.to_string());
            keywords.push(keyword.to_lowercase());
        }

        // Add CLI commands
        for cmd in CLI_COMMANDS {
            keywords.push(cmd.to_string());
        }

        keywords.sort();
        keywords.dedup();

        Self { keywords }
    }
}

impl Completer for CypherCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        // Find the word under cursor
        let start = line[..pos]
            .rfind(|c: char| c.is_whitespace() || c == '(' || c == ',' || c == '[')
            .map(|i| i + 1)
            .unwrap_or(0);

        let prefix = &line[start..pos];

        if prefix.is_empty() {
            return Ok((start, Vec::new()));
        }

        let prefix_lower = prefix.to_lowercase();
        let matches: Vec<Pair> = self
            .keywords
            .iter()
            .filter(|k| k.to_lowercase().starts_with(&prefix_lower))
            .map(|k| Pair {
                display: k.clone(),
                replacement: k.clone(),
            })
            .collect();

        Ok((start, matches))
    }
}

pub struct CypherValidator;

impl Validator for CypherValidator {
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        let input = ctx.input().trim();

        // Allow empty input
        if input.is_empty() {
            return Ok(ValidationResult::Valid(None));
        }

        // CLI commands are always complete
        if input.starts_with(':') {
            return Ok(ValidationResult::Valid(None));
        }

        // Check if query ends with semicolon
        if input.ends_with(';') {
            Ok(ValidationResult::Valid(None))
        } else {
            // Allow incomplete multi-line queries
            Ok(ValidationResult::Incomplete)
        }
    }
}
