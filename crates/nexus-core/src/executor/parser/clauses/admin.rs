//! Schema and administration clause parsers: CREATE/DROP/ALTER DATABASE,
//! CREATE/DROP INDEX, CREATE/DROP CONSTRAINT, CREATE/DROP USER, SHOW USER,
//! CREATE/DROP FUNCTION, API KEY management, GRANT, REVOKE.

use super::super::CypherParser;
use super::super::ast::*;
use crate::Result;

impl CypherParser {
    /// Parse CREATE DATABASE clause
    /// Syntax: CREATE DATABASE name [IF NOT EXISTS]
    pub(super) fn parse_create_database_clause(&mut self) -> Result<CreateDatabaseClause> {
        self.expect_keyword("DATABASE")?;
        self.skip_whitespace();

        // Check for IF NOT EXISTS
        let if_not_exists = if self.peek_keyword("IF") {
            self.parse_keyword()?; // consume "IF"
            self.expect_keyword("NOT")?;
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        let name = self.parse_identifier()?;
        Ok(CreateDatabaseClause {
            name,
            if_not_exists,
        })
    }

    /// Parse DROP DATABASE clause
    /// Syntax: DROP DATABASE name [IF EXISTS]
    pub(super) fn parse_drop_database_clause(&mut self) -> Result<DropDatabaseClause> {
        self.expect_keyword("DATABASE")?;
        self.skip_whitespace();

        // Check for IF EXISTS
        let if_exists = if self.peek_keyword("IF") {
            self.parse_keyword()?; // consume "IF"
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        let name = self.parse_identifier()?;
        Ok(DropDatabaseClause { name, if_exists })
    }

    /// Parse ALTER DATABASE clause
    /// Syntax: ALTER DATABASE name SET ACCESS {READ WRITE | READ ONLY}
    ///         ALTER DATABASE name SET OPTION key value
    pub(super) fn parse_alter_database_clause(&mut self) -> Result<AlterDatabaseClause> {
        self.expect_keyword("DATABASE")?;
        self.skip_whitespace();

        let name = self.parse_identifier()?;
        self.skip_whitespace();

        self.expect_keyword("SET")?;
        self.skip_whitespace();

        // Parse alteration type
        let alteration = if self.peek_keyword("ACCESS") {
            self.parse_keyword()?; // consume "ACCESS"
            self.skip_whitespace();

            // Parse READ WRITE or READ ONLY
            self.expect_keyword("READ")?;
            self.skip_whitespace();

            let read_only = if self.peek_keyword("ONLY") {
                self.parse_keyword()?;
                true
            } else if self.peek_keyword("WRITE") {
                self.parse_keyword()?;
                false
            } else {
                return Err(self.error("Expected ONLY or WRITE after READ in ALTER DATABASE"));
            };

            DatabaseAlteration::SetAccess { read_only }
        } else if self.peek_keyword("OPTION") {
            self.parse_keyword()?; // consume "OPTION"
            self.skip_whitespace();

            let key = self.parse_identifier()?;
            self.skip_whitespace();

            // Parse value - can be identifier or number
            let value = if self.peek_char().map_or(false, |c| c.is_ascii_digit()) {
                // Parse as number and convert to string
                self.parse_number()?.to_string()
            } else {
                // Parse as identifier
                self.parse_identifier()?
            };

            DatabaseAlteration::SetOption { key, value }
        } else {
            return Err(self.error("Expected ACCESS or OPTION after SET in ALTER DATABASE"));
        };

        Ok(AlterDatabaseClause { name, alteration })
    }

    /// Parse USE DATABASE clause
    /// Syntax: USE DATABASE name
    pub(super) fn parse_use_database_clause(&mut self) -> Result<UseDatabaseClause> {
        self.expect_keyword("DATABASE")?;
        self.skip_whitespace();
        let name = self.parse_identifier()?;
        Ok(UseDatabaseClause { name })
    }

    /// Parse CREATE INDEX clause
    /// Syntax: CREATE [OR REPLACE] [SPATIAL] INDEX [IF NOT EXISTS] ON :Label(property)
    pub(super) fn parse_create_index_clause(&mut self) -> Result<CreateIndexClause> {
        // Check for OR REPLACE before INDEX
        let or_replace = if self.peek_keyword("OR") {
            self.parse_keyword()?; // consume "OR"
            self.expect_keyword("REPLACE")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        // Check for SPATIAL keyword
        let index_type = if self.peek_keyword("SPATIAL") {
            self.parse_keyword()?; // consume "SPATIAL"
            self.skip_whitespace();
            Some("spatial".to_string())
        } else {
            None
        };

        self.expect_keyword("INDEX")?;
        self.skip_whitespace();

        // phase6_opencypher-advanced-types §3 — optional index name
        // between `INDEX` and `FOR`, e.g.
        // `CREATE INDEX person_id FOR (p:Person) ON (p.tenantId, p.id)`.
        let name = if self.is_identifier_start()
            && !self.peek_keyword("IF")
            && !self.peek_keyword("FOR")
            && !self.peek_keyword("ON")
        {
            Some(self.parse_identifier()?)
        } else {
            None
        };
        self.skip_whitespace();

        // Check for IF NOT EXISTS
        let if_not_exists = if self.peek_keyword("IF") {
            self.parse_keyword()?; // consume "IF"
            self.expect_keyword("NOT")?;
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        // Two grammar shapes:
        //   legacy : ON :Label(property)
        //   modern : FOR (var:Label) ON (var.p1, var.p2, ...)
        // The modern form is the only one that supports composite
        // property lists (§3.6). The legacy form is kept for every
        // existing test and SDK call site that already emits it.
        let (label, properties) = if self.peek_keyword("FOR") {
            self.parse_keyword()?; // consume "FOR"
            self.skip_whitespace();
            self.expect_char('(')?;
            self.skip_whitespace();
            let var = self.parse_identifier()?;
            self.skip_whitespace();
            self.expect_char(':')?;
            let lbl = self.parse_identifier()?;
            self.skip_whitespace();
            self.expect_char(')')?;
            self.skip_whitespace();
            self.expect_keyword("ON")?;
            self.skip_whitespace();
            self.expect_char('(')?;
            let mut props = Vec::new();
            loop {
                self.skip_whitespace();
                let p_var = self.parse_identifier()?;
                if p_var != var {
                    return Err(self.error(&format!(
                        "CREATE INDEX: property prefix {p_var:?} does not match pattern variable \
                         {var:?}"
                    )));
                }
                self.expect_char('.')?;
                let prop = self.parse_identifier()?;
                props.push(prop);
                self.skip_whitespace();
                if self.peek_char() == Some(',') {
                    self.consume_char();
                    continue;
                }
                break;
            }
            self.skip_whitespace();
            self.expect_char(')')?;
            (lbl, props)
        } else {
            self.expect_keyword("ON")?;
            self.skip_whitespace();
            self.expect_char(':')?;
            let lbl = self.parse_identifier()?;
            self.skip_whitespace();
            self.expect_char('(')?;
            let prop = self.parse_identifier()?;
            self.expect_char(')')?;
            (lbl, vec![prop])
        };

        // phase6_rtree-index-core §7.5 — `USING RTREE` alias for
        // `CREATE [SPATIAL] INDEX`. Both forms register the same
        // index on `IndexManager::rtree`. Treating `USING RTREE`
        // as equivalent to the leading `SPATIAL` keyword keeps a
        // Neo4j-dialect script with `... USING RTREE` parsing
        // unchanged.
        self.skip_whitespace();
        let index_type = if self.peek_keyword("USING") {
            self.parse_keyword()?; // consume "USING"
            self.skip_whitespace();
            if self.peek_keyword("RTREE") {
                self.parse_keyword()?;
                Some("spatial".to_string())
            } else {
                let raw = self.parse_identifier()?;
                let lower = raw.to_lowercase();
                match lower.as_str() {
                    "rtree" | "spatial" => Some("spatial".to_string()),
                    other => {
                        return Err(self.error(&format!(
                            "CREATE INDEX: unknown USING <type> {other:?}; expected RTREE"
                        )));
                    }
                }
            }
        } else {
            index_type
        };

        let property = properties.first().cloned().unwrap_or_default();

        Ok(CreateIndexClause {
            name,
            label,
            property,
            properties,
            if_not_exists,
            or_replace,
            index_type,
        })
    }

    /// Parse DROP INDEX clause
    /// Syntax: DROP INDEX [IF EXISTS] ON :Label(property)
    pub(super) fn parse_drop_index_clause(&mut self) -> Result<DropIndexClause> {
        self.expect_keyword("INDEX")?;
        self.skip_whitespace();

        // Check for IF EXISTS
        let if_exists = if self.peek_keyword("IF") {
            self.parse_keyword()?; // consume "IF"
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        self.expect_keyword("ON")?;
        self.skip_whitespace();
        self.expect_char(':')?;
        let label = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char('(')?;
        let property = self.parse_identifier()?;
        self.expect_char(')')?;

        Ok(DropIndexClause {
            label,
            property,
            if_exists,
        })
    }

    /// Parse CREATE CONSTRAINT clause.
    ///
    /// Accepted forms:
    ///
    /// ```text
    /// // Legacy (Cypher 4.x):
    /// CREATE CONSTRAINT [IF NOT EXISTS] ON (n:Label) ASSERT n.p IS UNIQUE
    /// CREATE CONSTRAINT [IF NOT EXISTS] ON (n:Label) ASSERT n.p IS NOT NULL
    /// CREATE CONSTRAINT [IF NOT EXISTS] ON (n:Label) ASSERT EXISTS(n.p)
    ///
    /// // Cypher 25 — node scope:
    /// CREATE CONSTRAINT [<name>] [IF NOT EXISTS]
    ///     FOR (n:Label) REQUIRE n.p IS UNIQUE
    /// CREATE CONSTRAINT [<name>] [IF NOT EXISTS]
    ///     FOR (n:Label) REQUIRE n.p IS NOT NULL
    /// CREATE CONSTRAINT [<name>] [IF NOT EXISTS]
    ///     FOR (n:Label) REQUIRE (n.p1, n.p2, ...) IS NODE KEY
    /// CREATE CONSTRAINT [<name>] [IF NOT EXISTS]
    ///     FOR (n:Label) REQUIRE n.p IS :: INTEGER   // or FLOAT / STRING / BOOLEAN / BYTES / LIST / MAP
    ///
    /// // Cypher 25 — relationship scope:
    /// CREATE CONSTRAINT [<name>] [IF NOT EXISTS]
    ///     FOR ()-[r:TYPE]-() REQUIRE r.p IS NOT NULL
    /// CREATE CONSTRAINT [<name>] [IF NOT EXISTS]
    ///     FOR ()-[r:TYPE]-() REQUIRE r.p IS :: INTEGER
    /// ```
    pub(super) fn parse_create_constraint_clause(&mut self) -> Result<CreateConstraintClause> {
        self.expect_keyword("CONSTRAINT")?;
        self.skip_whitespace();

        // Optional constraint name (`CREATE CONSTRAINT <name> [IF NOT EXISTS] FOR ...`).
        // Only legal before `IF`, `FOR`, or `ON`. Identifiers that
        // collide with a keyword are handled by the keyword checks.
        let name = if self.is_identifier_start()
            && !self.peek_keyword("IF")
            && !self.peek_keyword("FOR")
            && !self.peek_keyword("ON")
        {
            Some(self.parse_identifier()?)
        } else {
            None
        };
        self.skip_whitespace();

        // IF NOT EXISTS
        let if_not_exists = if self.peek_keyword("IF") {
            self.parse_keyword()?; // IF
            self.expect_keyword("NOT")?;
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        if self.peek_keyword("FOR") {
            self.parse_create_constraint_for_form(name, if_not_exists)
        } else {
            // Legacy `ON (n:L) ASSERT ...` form — every output here
            // is a node-scope constraint.
            self.expect_keyword("ON")?;
            self.skip_whitespace();
            self.expect_char('(')?;
            let _variable = self.parse_identifier()?;
            self.skip_whitespace();
            self.expect_char(':')?;
            let label = self.parse_identifier()?;
            self.expect_char(')')?;
            self.skip_whitespace();
            self.expect_keyword("ASSERT")?;
            self.skip_whitespace();
            let (constraint_type, property) = self.parse_legacy_constraint_body()?;
            Ok(CreateConstraintClause {
                name,
                constraint_type,
                label,
                property: property.clone(),
                properties: vec![property],
                entity: ConstraintEntity::Node,
                property_type: None,
                if_not_exists,
            })
        }
    }

    /// Legacy `ASSERT n.p IS UNIQUE / IS NOT NULL / EXISTS(n.p)` body.
    fn parse_legacy_constraint_body(&mut self) -> Result<(ConstraintType, String)> {
        if self.peek_keyword("EXISTS") {
            self.parse_keyword()?;
            self.expect_char('(')?;
            let _var = self.parse_identifier()?;
            self.expect_char('.')?;
            let prop = self.parse_identifier()?;
            self.expect_char(')')?;
            return Ok((ConstraintType::Exists, prop));
        }
        let _var = self.parse_identifier()?;
        self.expect_char('.')?;
        let prop = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_keyword("IS")?;
        self.skip_whitespace();
        if self.peek_keyword("NOT") {
            self.parse_keyword()?;
            self.skip_whitespace();
            self.expect_keyword("NULL")?;
            Ok((ConstraintType::Exists, prop))
        } else {
            self.expect_keyword("UNIQUE")?;
            Ok((ConstraintType::Unique, prop))
        }
    }

    /// Cypher 25 `FOR (n:L) REQUIRE ...` and
    /// `FOR ()-[r:T]-() REQUIRE ...` forms.
    fn parse_create_constraint_for_form(
        &mut self,
        name: Option<String>,
        if_not_exists: bool,
    ) -> Result<CreateConstraintClause> {
        self.expect_keyword("FOR")?;
        self.skip_whitespace();

        // Entity scope: node pattern `(n:L)` or rel pattern `()-[r:T]-()`.
        let (entity, var_name, label_or_type) =
            if self.peek_char() == Some('(') && !self.peek_is_rel_after_lparen() {
                // Actually look at next char to decide. Both forms start with `(`:
                //   node pattern:  (n:L)
                //   rel pattern:   ()-[r:T]-()
                // We disambiguate by peeking past `(` for `)-[`.
                self.parse_constraint_node_pattern()?
            } else {
                self.parse_constraint_rel_pattern()?
            };
        let _ = var_name;

        self.skip_whitespace();
        self.expect_keyword("REQUIRE")?;
        self.skip_whitespace();

        // Body: `(p1, p2, ...) IS NODE KEY` | `n.p IS UNIQUE` |
        //       `n.p IS NOT NULL` | `n.p IS :: TYPE`.
        let (constraint_type, properties, property_type) =
            if self.peek_char() == Some('(') && self.peek_is_node_key_tuple() {
                self.parse_require_node_key_body()?
            } else {
                let _var = self.parse_identifier()?;
                self.expect_char('.')?;
                let prop = self.parse_identifier()?;
                self.skip_whitespace();
                self.expect_keyword("IS")?;
                self.skip_whitespace();
                if self.peek_keyword("NOT") {
                    self.parse_keyword()?;
                    self.skip_whitespace();
                    self.expect_keyword("NULL")?;
                    (ConstraintType::Exists, vec![prop], None)
                } else if self.peek_char() == Some(':') && self.peek_char_at(1) == Some(':') {
                    self.consume_char();
                    self.consume_char();
                    self.skip_whitespace();
                    let ty = self.parse_identifier()?;
                    (ConstraintType::PropertyType, vec![prop], Some(ty))
                } else {
                    self.expect_keyword("UNIQUE")?;
                    (ConstraintType::Unique, vec![prop], None)
                }
            };

        let property = properties.first().cloned().unwrap_or_default();
        Ok(CreateConstraintClause {
            name,
            constraint_type,
            label: label_or_type,
            property,
            properties,
            entity,
            property_type,
            if_not_exists,
        })
    }

    /// Look past `(` to decide if the pattern is a node `(n:L)` or a
    /// relationship `()-[r:T]-()`. Stateless — `self.pos` is
    /// unchanged on return.
    fn peek_is_rel_after_lparen(&self) -> bool {
        let mut pos = self.pos + 1;
        // Skip whitespace inside `(`.
        while pos < self.input.len() {
            if !self.input.as_bytes()[pos].is_ascii_whitespace() {
                break;
            }
            pos += 1;
        }
        // Rel pattern shape: `()-[...`
        pos < self.input.len() && self.input.as_bytes()[pos] == b')'
    }

    /// Look past `(` to decide if we're at a NODE KEY tuple
    /// `(n.p1, n.p2)` vs a single `n.p` wrapped in parens. Heuristic:
    /// after the first `.`, a comma before the closing paren implies
    /// a tuple.
    fn peek_is_node_key_tuple(&self) -> bool {
        let mut depth = 0i32;
        for i in self.pos..self.input.len() {
            let b = self.input.as_bytes()[i];
            match b {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return false;
                    }
                }
                b',' if depth == 1 => return true,
                _ => {}
            }
        }
        false
    }

    fn parse_constraint_node_pattern(&mut self) -> Result<(ConstraintEntity, String, String)> {
        self.expect_char('(')?;
        self.skip_whitespace();
        let var = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char(':')?;
        let label = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char(')')?;
        Ok((ConstraintEntity::Node, var, label))
    }

    fn parse_constraint_rel_pattern(&mut self) -> Result<(ConstraintEntity, String, String)> {
        // Accepts `()-[r:TYPE]-()` and `()-[r:TYPE]->()`.
        self.expect_char('(')?;
        self.skip_whitespace();
        self.expect_char(')')?;
        self.skip_whitespace();
        self.expect_char('-')?;
        self.skip_whitespace();
        self.expect_char('[')?;
        self.skip_whitespace();
        let var = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char(':')?;
        let rel_type = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char(']')?;
        self.skip_whitespace();
        self.expect_char('-')?;
        self.skip_whitespace();
        if self.peek_char() == Some('>') {
            self.consume_char();
            self.skip_whitespace();
        }
        self.expect_char('(')?;
        self.skip_whitespace();
        self.expect_char(')')?;
        Ok((ConstraintEntity::Relationship, var, rel_type))
    }

    fn parse_require_node_key_body(
        &mut self,
    ) -> Result<(ConstraintType, Vec<String>, Option<String>)> {
        self.expect_char('(')?;
        let mut props = Vec::new();
        loop {
            self.skip_whitespace();
            let _var = self.parse_identifier()?;
            self.expect_char('.')?;
            props.push(self.parse_identifier()?);
            self.skip_whitespace();
            if self.peek_char() == Some(',') {
                self.consume_char();
                continue;
            }
            break;
        }
        self.skip_whitespace();
        self.expect_char(')')?;
        self.skip_whitespace();
        self.expect_keyword("IS")?;
        self.skip_whitespace();
        self.expect_keyword("NODE")?;
        self.skip_whitespace();
        self.expect_keyword("KEY")?;
        Ok((ConstraintType::NodeKey, props, None))
    }

    /// Parse DROP CONSTRAINT clause
    /// Syntax: DROP CONSTRAINT [IF EXISTS] ON (n:Label) ASSERT n.property IS UNIQUE
    pub(super) fn parse_drop_constraint_clause(&mut self) -> Result<DropConstraintClause> {
        self.expect_keyword("CONSTRAINT")?;
        self.skip_whitespace();

        // Check for IF EXISTS
        let if_exists = if self.peek_keyword("IF") {
            self.parse_keyword()?; // consume "IF"
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        self.expect_keyword("ON")?;
        self.skip_whitespace();
        self.expect_char('(')?;
        let _variable = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char(':')?;
        let label = self.parse_identifier()?;
        self.expect_char(')')?;
        self.skip_whitespace();
        self.expect_keyword("ASSERT")?;
        self.skip_whitespace();

        // Parse constraint type and extract property name (same as CREATE).
        // Accepts `IS UNIQUE`, `IS NOT NULL`, and the legacy `EXISTS(n.p)`.
        let (constraint_type, property) = if self.peek_keyword("EXISTS") {
            self.parse_keyword()?;
            self.expect_char('(')?;
            let _var = self.parse_identifier()?;
            self.expect_char('.')?;
            let prop = self.parse_identifier()?;
            self.expect_char(')')?;
            (ConstraintType::Exists, prop)
        } else {
            self.parse_identifier()?; // variable
            self.expect_char('.')?;
            let prop = self.parse_identifier()?;
            self.skip_whitespace();
            self.expect_keyword("IS")?;
            self.skip_whitespace();
            if self.peek_keyword("NOT") {
                self.parse_keyword()?;
                self.skip_whitespace();
                self.expect_keyword("NULL")?;
                (ConstraintType::Exists, prop)
            } else {
                self.expect_keyword("UNIQUE")?;
                (ConstraintType::Unique, prop)
            }
        };

        Ok(DropConstraintClause {
            constraint_type,
            label,
            property,
            if_exists,
        })
    }

    /// Parse CREATE USER clause
    /// Syntax: CREATE USER username [SET PASSWORD 'password'] [IF NOT EXISTS]
    pub(super) fn parse_create_user_clause(&mut self) -> Result<CreateUserClause> {
        self.expect_keyword("USER")?;
        self.skip_whitespace();

        // Check for IF NOT EXISTS first (it can come before username in some dialects)
        let mut if_not_exists = false;
        if self.peek_keyword("IF") {
            self.parse_keyword()?;
            self.expect_keyword("NOT")?;
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            if_not_exists = true;
        }

        let username = self.parse_identifier()?;
        self.skip_whitespace();

        // Check for SET PASSWORD
        let password = if self.peek_keyword("SET") {
            self.parse_keyword()?;
            self.expect_keyword("PASSWORD")?;
            self.skip_whitespace();
            let pwd_expr = self.parse_string_literal()?;
            // Extract string value from Expression::Literal(Literal::String)
            if let Expression::Literal(crate::executor::parser::Literal::String(s)) = pwd_expr {
                Some(s)
            } else {
                return Err(self.error("PASSWORD must be a string literal"));
            }
        } else {
            None
        };

        // Check for IF NOT EXISTS after username
        if !if_not_exists && self.peek_keyword("IF") {
            self.parse_keyword()?;
            self.expect_keyword("NOT")?;
            self.expect_keyword("EXISTS")?;
            if_not_exists = true;
        }

        Ok(CreateUserClause {
            username,
            password,
            if_not_exists,
        })
    }

    /// Parse DROP USER clause
    /// Syntax: DROP USER username [IF EXISTS]
    pub(super) fn parse_drop_user_clause(&mut self) -> Result<DropUserClause> {
        self.expect_keyword("USER")?;
        self.skip_whitespace();

        // Check for IF EXISTS first
        let mut if_exists = false;
        if self.peek_keyword("IF") {
            self.parse_keyword()?;
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            if_exists = true;
        }

        let username = self.parse_identifier()?;
        self.skip_whitespace();

        // Check for IF EXISTS after username
        if !if_exists && self.peek_keyword("IF") {
            self.parse_keyword()?;
            self.expect_keyword("EXISTS")?;
            if_exists = true;
        }

        Ok(DropUserClause {
            username,
            if_exists,
        })
    }

    /// Parse SHOW USER clause
    /// Syntax: SHOW USER username
    pub(super) fn parse_show_user_clause(&mut self) -> Result<ShowUserClause> {
        self.expect_keyword("USER")?;
        self.skip_whitespace();
        let username = self.parse_identifier()?;
        Ok(ShowUserClause { username })
    }

    /// Parse CREATE FUNCTION clause
    /// Syntax: CREATE FUNCTION name(param1: Type1, param2: Type2) [IF NOT EXISTS] RETURNS Type [AS expression]
    /// Note: For MVP, we'll use a simplified syntax that stores the signature only
    /// The actual function implementation must be registered via API/plugin system
    pub(super) fn parse_create_function_clause(&mut self) -> Result<CreateFunctionClause> {
        self.expect_keyword("FUNCTION")?;
        self.skip_whitespace();

        // Check for IF NOT EXISTS
        let mut if_not_exists = false;
        if self.peek_keyword("IF") {
            self.parse_keyword()?;
            self.expect_keyword("NOT")?;
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            if_not_exists = true;
        }

        // Parse function name
        let name = self.parse_identifier()?;
        self.skip_whitespace();

        // Parse parameters: (param1: Type1, param2: Type2)
        let mut parameters = Vec::new();
        self.expect_char('(')?;
        self.skip_whitespace();

        if self.peek_char() != Some(')') {
            loop {
                let param_name = self.parse_identifier()?;
                self.skip_whitespace();
                self.expect_char(':')?;
                self.skip_whitespace();

                // Parse parameter type
                let type_str = self.parse_identifier()?;
                let param_type = match type_str.to_lowercase().as_str() {
                    "integer" | "int" => crate::udf::UdfReturnType::Integer,
                    "float" | "double" => crate::udf::UdfReturnType::Float,
                    "string" | "str" => crate::udf::UdfReturnType::String,
                    "boolean" | "bool" => crate::udf::UdfReturnType::Boolean,
                    "any" => crate::udf::UdfReturnType::Any,
                    _ => {
                        return Err(self.error(&format!("Unknown parameter type: {}", type_str)));
                    }
                };

                parameters.push(UdfParameter {
                    name: param_name,
                    param_type: param_type.clone(),
                    required: true, // For MVP, all parameters are required
                    default: None,
                });

                self.skip_whitespace();
                if self.peek_char() == Some(',') {
                    self.consume_char();
                    self.skip_whitespace();
                } else if self.peek_char() == Some(')') {
                    break;
                } else {
                    return Err(self.error("Expected ',' or ')' in function parameters"));
                }
            }
        }

        self.expect_char(')')?;
        self.skip_whitespace();

        // Parse RETURNS type
        self.expect_keyword("RETURNS")?;
        self.skip_whitespace();
        let return_type_str = self.parse_identifier()?;
        let return_type = match return_type_str.to_lowercase().as_str() {
            "integer" | "int" => crate::udf::UdfReturnType::Integer,
            "float" | "double" => crate::udf::UdfReturnType::Float,
            "string" | "str" => crate::udf::UdfReturnType::String,
            "boolean" | "bool" => crate::udf::UdfReturnType::Boolean,
            "any" => crate::udf::UdfReturnType::Any,
            _ => {
                return Err(self.error(&format!("Unknown return type: {}", return_type_str)));
            }
        };

        self.skip_whitespace();

        // Parse optional description (AS 'description')
        let mut description = None;
        if self.peek_keyword("AS") {
            self.parse_keyword()?;
            self.skip_whitespace();
            if self.peek_char() == Some('\'') || self.peek_char() == Some('"') {
                let desc_str = self.parse_string_literal()?;
                if let Expression::Literal(crate::executor::parser::Literal::String(s)) = desc_str {
                    description = Some(s);
                }
            }
        }

        Ok(CreateFunctionClause {
            name,
            parameters,
            return_type,
            if_not_exists,
            description,
        })
    }

    /// Parse DROP FUNCTION clause
    /// Syntax: DROP FUNCTION name [IF EXISTS]
    pub(super) fn parse_drop_function_clause(&mut self) -> Result<DropFunctionClause> {
        self.expect_keyword("FUNCTION")?;
        self.skip_whitespace();

        // Check for IF EXISTS
        let mut if_exists = false;
        if self.peek_keyword("IF") {
            self.parse_keyword()?;
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            if_exists = true;
        }

        let name = self.parse_identifier()?;
        self.skip_whitespace();

        // Check for IF EXISTS after function name
        if !if_exists && self.peek_keyword("IF") {
            self.parse_keyword()?;
            self.expect_keyword("EXISTS")?;
            if_exists = true;
        }

        Ok(DropFunctionClause { name, if_exists })
    }

    /// Parse CREATE API KEY clause
    /// Syntax: CREATE API KEY name [FOR username] [WITH PERMISSIONS ...] [EXPIRES IN 'duration']
    pub(super) fn parse_create_api_key_clause(&mut self) -> Result<CreateApiKeyClause> {
        self.skip_whitespace();
        let name = self.parse_identifier()?;
        self.skip_whitespace();

        let mut user_id = None;
        let mut permissions = Vec::new();
        let mut expires_in = None;

        // Parse optional FOR username
        if self.peek_keyword("FOR") {
            self.parse_keyword()?;
            self.skip_whitespace();
            user_id = Some(self.parse_identifier()?);
            self.skip_whitespace();
        }

        // Parse optional WITH PERMISSIONS
        if self.peek_keyword("WITH") {
            self.parse_keyword()?;
            self.expect_keyword("PERMISSIONS")?;
            self.skip_whitespace();
            loop {
                let permission = self.parse_identifier()?;
                permissions.push(permission);
                self.skip_whitespace();
                if self.peek_char() == Some(',') {
                    self.consume_char();
                    self.skip_whitespace();
                } else {
                    break;
                }
            }
        }

        // Parse optional EXPIRES IN
        if self.peek_keyword("EXPIRES") {
            self.parse_keyword()?;
            self.expect_keyword("IN")?;
            self.skip_whitespace();
            let duration_str = self.parse_string_literal()?;
            if let Expression::Literal(crate::executor::parser::Literal::String(s)) = duration_str {
                expires_in = Some(s);
            }
        }

        Ok(CreateApiKeyClause {
            name,
            user_id,
            permissions,
            expires_in,
        })
    }

    /// Parse SHOW API KEYS clause
    /// Syntax: SHOW API KEYS [FOR username]
    pub(super) fn parse_show_api_keys_clause(&mut self) -> Result<ShowApiKeysClause> {
        self.skip_whitespace();
        let mut user_id = None;

        if self.peek_keyword("FOR") {
            self.parse_keyword()?;
            self.skip_whitespace();
            user_id = Some(self.parse_identifier()?);
        }

        Ok(ShowApiKeysClause { user_id })
    }

    /// Parse REVOKE API KEY clause
    /// Syntax: REVOKE API KEY 'key_id' [REASON 'reason']
    pub(super) fn parse_revoke_api_key_clause(&mut self) -> Result<RevokeApiKeyClause> {
        self.skip_whitespace();
        let key_id_str = self.parse_string_literal()?;
        let key_id =
            if let Expression::Literal(crate::executor::parser::Literal::String(s)) = key_id_str {
                s
            } else {
                return Err(self.error("API key ID must be a string literal"));
            };

        self.skip_whitespace();
        let mut reason = None;

        if self.peek_keyword("REASON") {
            self.parse_keyword()?;
            self.skip_whitespace();
            let reason_str = self.parse_string_literal()?;
            if let Expression::Literal(crate::executor::parser::Literal::String(s)) = reason_str {
                reason = Some(s);
            }
        }

        Ok(RevokeApiKeyClause { key_id, reason })
    }

    /// Parse DELETE API KEY clause
    /// Syntax: DELETE API KEY 'key_id'
    pub(super) fn parse_delete_api_key_clause(&mut self) -> Result<DeleteApiKeyClause> {
        self.skip_whitespace();
        let key_id_str = self.parse_string_literal()?;
        let key_id =
            if let Expression::Literal(crate::executor::parser::Literal::String(s)) = key_id_str {
                s
            } else {
                return Err(self.error("API key ID must be a string literal"));
            };

        Ok(DeleteApiKeyClause { key_id })
    }

    /// Parse GRANT clause
    /// Syntax: GRANT permission [, permission ...] TO target
    pub(super) fn parse_grant_clause(&mut self) -> Result<GrantClause> {
        self.skip_whitespace();

        // Parse permissions
        let mut permissions = Vec::new();
        loop {
            let permission = self.parse_identifier()?;
            permissions.push(permission);
            self.skip_whitespace();
            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            } else {
                break;
            }
        }

        self.expect_keyword("TO")?;
        self.skip_whitespace();
        let target = self.parse_identifier()?;

        Ok(GrantClause {
            permissions,
            target,
        })
    }

    /// Parse REVOKE clause
    /// Syntax: REVOKE permission [, permission ...] FROM target
    pub(super) fn parse_revoke_clause(&mut self) -> Result<RevokeClause> {
        self.skip_whitespace();

        // Parse permissions
        let mut permissions = Vec::new();
        loop {
            let permission = self.parse_identifier()?;
            permissions.push(permission);
            self.skip_whitespace();
            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            } else {
                break;
            }
        }

        self.expect_keyword("FROM")?;
        self.skip_whitespace();
        let target = self.parse_identifier()?;

        Ok(RevokeClause {
            permissions,
            target,
        })
    }
}
