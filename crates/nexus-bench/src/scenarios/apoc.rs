//! APOC-namespace scenarios. Every query calls a procedure Nexus ships
//! in-tree (`crate::apoc::*`) that also exists in the Neo4j APOC
//! distribution, so the `live_compare` harness diffs result rows
//! against a real Neo4j peer. The shape is deliberately
//! `CALL apoc.x.y(...) YIELD value RETURN value` so drivers see a
//! single named column — the shape APOC itself uses.

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

pub(crate) fn scenarios() -> Vec<Scenario> {
    let mut out = Vec::new();
    out.extend(coll());
    out.extend(map());
    out.extend(text());
    out.extend(date());
    out.extend(util());
    out.extend(convert());
    out.extend(number());
    out.extend(agg());
    out
}

fn call_value(id: &str, description: &str, query: &str) -> Scenario {
    ScenarioBuilder::new(id, description, DatasetKind::Tiny, query)
        .expected_rows(1)
        .build()
}

fn coll() -> Vec<Scenario> {
    vec![
        call_value(
            "apoc.coll.union",
            "apoc.coll.union([1,2,3], [3,4,5])",
            "CALL apoc.coll.union([1,2,3], [3,4,5]) YIELD value RETURN value",
        ),
        call_value(
            "apoc.coll.intersection",
            "apoc.coll.intersection — set intersection",
            "CALL apoc.coll.intersection([1,2,3,4], [3,4,5]) YIELD value RETURN value",
        ),
        call_value(
            "apoc.coll.sort",
            "apoc.coll.sort — natural ascending",
            "CALL apoc.coll.sort([3,1,4,1,5,9,2,6]) YIELD value RETURN value",
        ),
        call_value(
            "apoc.coll.flatten",
            "apoc.coll.flatten — shallow",
            "CALL apoc.coll.flatten([[1,2],[3,[4,5]]]) YIELD value RETURN value",
        ),
        call_value(
            "apoc.coll.sum",
            "apoc.coll.sum — arithmetic reduction",
            "CALL apoc.coll.sum([1,2,3,4,5]) YIELD value RETURN value",
        ),
        call_value(
            "apoc.coll.avg",
            "apoc.coll.avg — mean reduction",
            "CALL apoc.coll.avg([1,2,3,4,5]) YIELD value RETURN value",
        ),
        call_value(
            "apoc.coll.frequencies",
            "apoc.coll.frequencies — count-desc histogram",
            "CALL apoc.coll.frequencies(['a','b','a','c','a','b']) YIELD value RETURN value",
        ),
        call_value(
            "apoc.coll.zip",
            "apoc.coll.zip — pointwise pair",
            "CALL apoc.coll.zip([1,2,3], ['a','b','c']) YIELD value RETURN value",
        ),
    ]
}

fn map() -> Vec<Scenario> {
    vec![
        call_value(
            "apoc.map.merge",
            "apoc.map.merge — right-wins",
            "CALL apoc.map.merge({a:1, b:2}, {b:3, c:4}) YIELD value RETURN value",
        ),
        call_value(
            "apoc.map.fromPairs",
            "apoc.map.fromPairs — LIST<[k,v]> → MAP",
            "CALL apoc.map.fromPairs([['a',1],['b',2]]) YIELD value RETURN value",
        ),
        call_value(
            "apoc.map.removeKeys",
            "apoc.map.removeKeys — drop listed keys",
            "CALL apoc.map.removeKeys({a:1, b:2, c:3}, ['a','b']) YIELD value RETURN value",
        ),
        call_value(
            "apoc.map.flatten",
            "apoc.map.flatten — nested to dotted",
            "CALL apoc.map.flatten({a:{b:{c:1}}, d:2}) YIELD value RETURN value",
        ),
    ]
}

fn text() -> Vec<Scenario> {
    vec![
        call_value(
            "apoc.text.levenshteinDistance",
            "apoc.text.levenshteinDistance — classic kitten/sitting",
            "CALL apoc.text.levenshteinDistance('kitten', 'sitting') YIELD value RETURN value",
        ),
        call_value(
            "apoc.text.jaroWinklerDistance",
            "apoc.text.jaroWinklerDistance — identical strings",
            "CALL apoc.text.jaroWinklerDistance('abc', 'abc') YIELD value RETURN value",
        ),
        call_value(
            "apoc.text.regexGroups",
            "apoc.text.regexGroups — capture groups",
            "CALL apoc.text.regexGroups('abc 123', '(\\\\w+) (\\\\d+)') YIELD value RETURN value",
        ),
        call_value(
            "apoc.text.camelCase",
            "apoc.text.camelCase — tokenised join",
            "CALL apoc.text.camelCase('hello world foo bar') YIELD value RETURN value",
        ),
        call_value(
            "apoc.text.lpad",
            "apoc.text.lpad — zero-pad integer",
            "CALL apoc.text.lpad('42', 5, '0') YIELD value RETURN value",
        ),
    ]
}

fn date() -> Vec<Scenario> {
    vec![
        call_value(
            "apoc.date.format",
            "apoc.date.format — ms → yyyy-MM-dd",
            "CALL apoc.date.format(1610668800000, 'ms', 'yyyy-MM-dd') YIELD value RETURN value",
        ),
        call_value(
            "apoc.date.parse",
            "apoc.date.parse — STRING → ms",
            "CALL apoc.date.parse('2021-01-15 12:30:45', 'ms', 'yyyy-MM-dd HH:mm:ss') YIELD value RETURN value",
        ),
        call_value(
            "apoc.date.toDays",
            "apoc.date.toDays — ms bucket",
            "CALL apoc.date.toDays(172800000, 'ms') YIELD value RETURN value",
        ),
        call_value(
            "apoc.date.fromISO",
            "apoc.date.fromISO — RFC3339 → ms",
            "CALL apoc.date.fromISO('2021-01-15T00:00:00Z') YIELD value RETURN value",
        ),
    ]
}

fn util() -> Vec<Scenario> {
    vec![
        call_value(
            "apoc.util.md5",
            "apoc.util.md5 — RFC 1321 digest",
            "CALL apoc.util.md5(['abc']) YIELD value RETURN value",
        ),
        call_value(
            "apoc.util.sha256",
            "apoc.util.sha256 — FIPS 180-4 digest",
            "CALL apoc.util.sha256(['abc']) YIELD value RETURN value",
        ),
        call_value(
            "apoc.util.uuid",
            "apoc.util.uuid — v4 generator",
            "CALL apoc.util.uuid() YIELD value RETURN size(value) AS len",
        ),
    ]
}

fn convert() -> Vec<Scenario> {
    vec![
        call_value(
            "apoc.convert.toJson",
            "apoc.convert.toJson — MAP → STRING",
            "CALL apoc.convert.toJson({a:1,b:[2,3]}) YIELD value RETURN value",
        ),
        call_value(
            "apoc.convert.fromJsonMap",
            "apoc.convert.fromJsonMap — STRING → MAP",
            "CALL apoc.convert.fromJsonMap('{\"a\":1}') YIELD value RETURN value",
        ),
        call_value(
            "apoc.convert.toInteger",
            "apoc.convert.toInteger — STRING → INT",
            "CALL apoc.convert.toInteger('42') YIELD value RETURN value",
        ),
    ]
}

fn number() -> Vec<Scenario> {
    vec![
        call_value(
            "apoc.number.format",
            "apoc.number.format — thousands-separated",
            "CALL apoc.number.format(1234567.89) YIELD value RETURN value",
        ),
        call_value(
            "apoc.number.arabicToRoman",
            "apoc.number.arabicToRoman — 1994 → MCMXCIV",
            "CALL apoc.number.arabicToRoman(1994) YIELD value RETURN value",
        ),
        call_value(
            "apoc.number.romanToArabic",
            "apoc.number.romanToArabic — MCMXCIV → 1994",
            "CALL apoc.number.romanToArabic('MCMXCIV') YIELD value RETURN value",
        ),
    ]
}

fn agg() -> Vec<Scenario> {
    vec![
        call_value(
            "apoc.agg.statistics",
            "apoc.agg.statistics — full stat bundle",
            "CALL apoc.agg.statistics([1,2,3,4,5,6,7,8,9,10]) YIELD value RETURN value",
        ),
        call_value(
            "apoc.agg.median",
            "apoc.agg.median — middle value",
            "CALL apoc.agg.median([1,2,3,4,5]) YIELD value RETURN value",
        ),
        call_value(
            "apoc.agg.product",
            "apoc.agg.product — multiplicative fold",
            "CALL apoc.agg.product([2,3,4]) YIELD value RETURN value",
        ),
    ]
}
