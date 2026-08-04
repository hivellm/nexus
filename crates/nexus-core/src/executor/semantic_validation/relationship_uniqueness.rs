//! Relationship-uniqueness check: one `MATCH` pattern may not name the same
//! relationship variable twice.
//!
//! Cypher scopes relationship isomorphism to a clause, so two slots of one
//! pattern always bind two DIFFERENT relationships — which makes a repeated
//! relationship variable unsatisfiable by construction. openCypher does not
//! return zero rows for it; it rejects the query. Source: openCypher TCK
//! `clauses/match/Match3.feature` scenario [29] "Fail when re-using a
//! relationship in the same pattern" — `MATCH (a)-[r]->()-[r]->(a) RETURN r`
//! must raise a `SyntaxError` at compile time with the detail token
//! `RelationshipUniquenessViolation`.
//!
//! # Scope, and why it is deliberately narrow
//!
//! Following this pass's conservative-over-collection principle (see the
//! parent module's docs), the check flags only what the rule provably covers:
//!
//! - **Per `MATCH` pattern.** Reusing a relationship variable in a LATER
//!   clause is legal Cypher — it joins on that relationship, as openCypher TCK
//!   `MatchWhere6` [5] does (`MATCH (a1)-[r]->() WITH r, a1 … OPTIONAL MATCH
//!   (a2)<-[r]-(b2)`). Only repeats WITHIN one pattern are violations.
//! - **`MATCH` only.** `CREATE`/`MERGE` patterns rebinding a relationship
//!   variable are a different rule (`VariableAlreadyBound`), already handled by
//!   `check_variable_already_bound`.
//! - **Top-level relationship slots only.** A variable declared inside a
//!   quantified path pattern group is left alone; missing a violation there is
//!   a safe false negative, inventing one is not.
//!
//! Unlike most of this module the check runs even on queries the parent gate
//! calls un-modeled (`UNION`, `CALL {…}`, …): it is purely syntactic per
//! pattern and cannot be affected by cross-clause scoping.

use std::collections::HashSet;

use crate::executor::parser::ast::{Clause, CypherQuery, Pattern, PatternElement};

/// Reject any `MATCH` pattern that names one relationship variable twice.
pub(super) fn check_relationship_uniqueness(query: &CypherQuery) -> crate::Result<()> {
    for clause in &query.clauses {
        if let Clause::Match(match_clause) = clause {
            check_pattern(&match_clause.pattern)?;
        }
    }
    Ok(())
}

fn check_pattern(pattern: &Pattern) -> crate::Result<()> {
    let mut seen: HashSet<&str> = HashSet::new();
    for element in &pattern.elements {
        if let PatternElement::Relationship(rel) = element
            && let Some(var) = &rel.variable
            && !seen.insert(var.as_str())
        {
            return Err(relationship_uniqueness_violation(var));
        }
    }
    Ok(())
}

/// A `SyntaxError` carrying openCypher's `RelationshipUniquenessViolation`
/// detail token, in the same CamelCase-token-in-message form the rest of this
/// pass uses so the conformance runner can classify it.
fn relationship_uniqueness_violation(name: &str) -> crate::Error {
    crate::Error::CypherSyntax(format!(
        "RelationshipUniquenessViolation: relationship variable `{name}` cannot be \
         re-used within the same pattern — two relationship slots of one MATCH \
         always bind different relationships"
    ))
}

#[cfg(test)]
mod tests {
    use crate::executor::parser::CypherParser;

    fn run(query: &str) -> crate::Result<()> {
        let ast = CypherParser::new(query.to_string()).parse()?;
        super::super::validate(&ast)
    }

    /// The openCypher TCK `clauses/match/Match3.feature` [29] query itself.
    #[test]
    fn rejects_the_tck_reuse_scenario() {
        let err = run("MATCH (a)-[r]->()-[r]->(a) RETURN r")
            .expect_err("re-using `r` in one pattern must be rejected");
        assert!(
            err.to_string().contains("RelationshipUniquenessViolation"),
            "message must carry the detail token, got: {err}"
        );
    }

    #[test]
    fn rejects_a_repeat_across_comma_separated_parts_of_one_clause() {
        // Comma parts live in the same pattern, and isomorphism spans them, so
        // the repeat is just as unsatisfiable there.
        let err = run("MATCH (a)-[r]->(b), (c)-[r]->(d) RETURN r")
            .expect_err("comma parts are the same pattern");
        assert!(err.to_string().contains("RelationshipUniquenessViolation"));
    }

    #[test]
    fn accepts_distinct_relationship_variables() {
        run("MATCH (a)-[r1]->()-[r2]->(a) RETURN r1, r2").expect("distinct slots are legal");
    }

    #[test]
    fn accepts_a_relationship_variable_re_used_by_a_later_clause() {
        // openCypher TCK `MatchWhere6` [5] shape: reusing `r` in a later clause
        // joins on that relationship and is legal.
        run("MATCH (a1)-[r]->() WITH r, a1 OPTIONAL MATCH (a2)<-[r]-(b2) RETURN a1, r, b2, a2")
            .expect("cross-clause re-use is legal");
    }

    #[test]
    fn accepts_anonymous_slots_repeated() {
        run("MATCH (a)-[]->()-[]->(a) RETURN a").expect("anonymous slots bind no variable");
    }
}
