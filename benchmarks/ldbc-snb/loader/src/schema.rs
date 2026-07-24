//! Declarative description of the LDBC SNB `CsvCompositeMergeForeign` /
//! `LongDateFormatter` layout: which file produces which label, how each
//! column is coerced, and which foreign-key columns have to be synthesized
//! into relationships the serializer does not emit as their own files.
//!
//! Keeping this as data rather than code means the loader's two passes (nodes,
//! then merge-foreign edges) read the SAME description, so a column can never
//! be a property in one pass and a foreign key in the other.

/// How a CSV column becomes a Nexus property value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coerce {
    /// Parse as `i64` — ids, lengths, and every date/datetime field.
    ///
    /// Dates stay EPOCH MILLISECONDS, exactly as `LongDateFormatter` writes
    /// them, rather than being converted to an ISO-8601 string. Three reasons:
    /// Nexus has no native temporal property type (`datetime()` returns a
    /// string); the benchmark's own substitution parameters express dates as
    /// epoch millis too, so queries compare like with like; and integer
    /// comparisons are served by the property B-tree's range seek, which a
    /// string encoding would only match by lexicographic accident.
    Int,
    /// Keep as a string.
    Str,
    /// Split on `;` into an array of strings (`person.language`,
    /// `person.email`). An empty field yields an empty array.
    StrList,
}

/// One property column of a node file.
#[derive(Debug, Clone, Copy)]
pub struct Property {
    /// Column name in the CSV header.
    pub column: &'static str,
    /// Property name to store it under (differs from `column` only where the
    /// CSV name is not a legal/meaningful property name).
    pub name: &'static str,
    pub coerce: Coerce,
}

/// Direction of a synthesized merge-foreign relationship relative to the row
/// that carries the foreign key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FkDirection {
    /// `(this row's node)-[:TYPE]->(referenced node)`.
    Outgoing,
    /// `(referenced node)-[:TYPE]->(this row's node)` — used by
    /// `CONTAINER_OF`, which the SNB schema orients Forum → Post while the FK
    /// lives on the Post row.
    Incoming,
}

/// A foreign-key column that must become a relationship, because the
/// `MergeForeign` serializer folds single-cardinality edges into the owning
/// node's CSV instead of emitting an edge file.
#[derive(Debug, Clone, Copy)]
pub struct ForeignKey {
    /// Column holding the referenced node's LDBC id. An empty value means "no
    /// edge" (continents have no `isPartOf`, the root TagClass no
    /// `isSubclassOf`, and `replyOfPost`/`replyOfComment` are mutually
    /// exclusive) — never an edge to a null target.
    pub column: &'static str,
    /// Label of the referenced node, needed because LDBC ids are only unique
    /// WITHIN a label: `Place 0`, `Organisation 0`, `Tag 0` and `Forum 0` all
    /// exist in SF0.1.
    pub target_label: &'static str,
    pub rel_type: &'static str,
    pub direction: FkDirection,
}

/// A node CSV: one file, one label.
#[derive(Debug, Clone, Copy)]
pub struct NodeFile {
    /// Path relative to the dataset root.
    pub path: &'static str,
    pub label: &'static str,
    /// Column holding the LDBC id (always `id` in this layout, but named here
    /// so the loader never hard-codes a column position).
    pub id_column: &'static str,
    pub properties: &'static [Property],
    pub foreign_keys: &'static [ForeignKey],
}

/// An edge CSV: one file, one relationship type.
#[derive(Debug, Clone, Copy)]
pub struct EdgeFile {
    pub path: &'static str,
    pub rel_type: &'static str,
    /// Header name + label of the source column.
    pub src: (&'static str, &'static str),
    /// Header name + label of the destination column. `person_knows_person`
    /// repeats the `Person.id` header, so columns are addressed by INDEX
    /// (0 and 1), with these names kept for error messages only.
    pub dst: (&'static str, &'static str),
    pub properties: &'static [Property],
    /// Materialize the mirrored edge as well. `person_knows_person` is
    /// undirected and stored once per pair; without the mirror, half of every
    /// friendship traversal silently disappears.
    pub undirected: bool,
}

pub const NODE_FILES: &[NodeFile] = &[
    NodeFile {
        path: "static/place_0_0.csv",
        label: "Place",
        id_column: "id",
        properties: &[
            Property {
                column: "id",
                name: "id",
                coerce: Coerce::Int,
            },
            Property {
                column: "name",
                name: "name",
                coerce: Coerce::Str,
            },
            Property {
                column: "url",
                name: "url",
                coerce: Coerce::Str,
            },
            Property {
                column: "type",
                name: "type",
                coerce: Coerce::Str,
            },
        ],
        foreign_keys: &[ForeignKey {
            column: "isPartOf",
            target_label: "Place",
            rel_type: "IS_PART_OF",
            direction: FkDirection::Outgoing,
        }],
    },
    NodeFile {
        path: "static/organisation_0_0.csv",
        label: "Organisation",
        id_column: "id",
        properties: &[
            Property {
                column: "id",
                name: "id",
                coerce: Coerce::Int,
            },
            Property {
                column: "type",
                name: "type",
                coerce: Coerce::Str,
            },
            Property {
                column: "name",
                name: "name",
                coerce: Coerce::Str,
            },
            Property {
                column: "url",
                name: "url",
                coerce: Coerce::Str,
            },
        ],
        foreign_keys: &[ForeignKey {
            column: "place",
            target_label: "Place",
            rel_type: "IS_LOCATED_IN",
            direction: FkDirection::Outgoing,
        }],
    },
    NodeFile {
        path: "static/tagclass_0_0.csv",
        label: "TagClass",
        id_column: "id",
        properties: &[
            Property {
                column: "id",
                name: "id",
                coerce: Coerce::Int,
            },
            Property {
                column: "name",
                name: "name",
                coerce: Coerce::Str,
            },
            Property {
                column: "url",
                name: "url",
                coerce: Coerce::Str,
            },
        ],
        foreign_keys: &[ForeignKey {
            column: "isSubclassOf",
            target_label: "TagClass",
            rel_type: "IS_SUBCLASS_OF",
            direction: FkDirection::Outgoing,
        }],
    },
    NodeFile {
        path: "static/tag_0_0.csv",
        label: "Tag",
        id_column: "id",
        properties: &[
            Property {
                column: "id",
                name: "id",
                coerce: Coerce::Int,
            },
            Property {
                column: "name",
                name: "name",
                coerce: Coerce::Str,
            },
            Property {
                column: "url",
                name: "url",
                coerce: Coerce::Str,
            },
        ],
        foreign_keys: &[ForeignKey {
            column: "hasType",
            target_label: "TagClass",
            rel_type: "HAS_TYPE",
            direction: FkDirection::Outgoing,
        }],
    },
    NodeFile {
        path: "dynamic/person_0_0.csv",
        label: "Person",
        id_column: "id",
        properties: &[
            Property {
                column: "id",
                name: "id",
                coerce: Coerce::Int,
            },
            Property {
                column: "firstName",
                name: "firstName",
                coerce: Coerce::Str,
            },
            Property {
                column: "lastName",
                name: "lastName",
                coerce: Coerce::Str,
            },
            Property {
                column: "gender",
                name: "gender",
                coerce: Coerce::Str,
            },
            Property {
                column: "birthday",
                name: "birthday",
                coerce: Coerce::Int,
            },
            Property {
                column: "creationDate",
                name: "creationDate",
                coerce: Coerce::Int,
            },
            Property {
                column: "locationIP",
                name: "locationIP",
                coerce: Coerce::Str,
            },
            Property {
                column: "browserUsed",
                name: "browserUsed",
                coerce: Coerce::Str,
            },
            Property {
                column: "language",
                name: "language",
                coerce: Coerce::StrList,
            },
            Property {
                column: "email",
                name: "email",
                coerce: Coerce::StrList,
            },
        ],
        foreign_keys: &[ForeignKey {
            column: "place",
            target_label: "Place",
            rel_type: "IS_LOCATED_IN",
            direction: FkDirection::Outgoing,
        }],
    },
    NodeFile {
        path: "dynamic/forum_0_0.csv",
        label: "Forum",
        id_column: "id",
        properties: &[
            Property {
                column: "id",
                name: "id",
                coerce: Coerce::Int,
            },
            Property {
                column: "title",
                name: "title",
                coerce: Coerce::Str,
            },
            Property {
                column: "creationDate",
                name: "creationDate",
                coerce: Coerce::Int,
            },
        ],
        foreign_keys: &[ForeignKey {
            column: "moderator",
            target_label: "Person",
            rel_type: "HAS_MODERATOR",
            direction: FkDirection::Outgoing,
        }],
    },
    NodeFile {
        path: "dynamic/post_0_0.csv",
        label: "Post",
        id_column: "id",
        properties: &[
            Property {
                column: "id",
                name: "id",
                coerce: Coerce::Int,
            },
            Property {
                column: "imageFile",
                name: "imageFile",
                coerce: Coerce::Str,
            },
            Property {
                column: "creationDate",
                name: "creationDate",
                coerce: Coerce::Int,
            },
            Property {
                column: "locationIP",
                name: "locationIP",
                coerce: Coerce::Str,
            },
            Property {
                column: "browserUsed",
                name: "browserUsed",
                coerce: Coerce::Str,
            },
            Property {
                column: "language",
                name: "language",
                coerce: Coerce::Str,
            },
            Property {
                column: "content",
                name: "content",
                coerce: Coerce::Str,
            },
            Property {
                column: "length",
                name: "length",
                coerce: Coerce::Int,
            },
        ],
        foreign_keys: &[
            ForeignKey {
                column: "creator",
                target_label: "Person",
                rel_type: "HAS_CREATOR",
                direction: FkDirection::Outgoing,
            },
            ForeignKey {
                column: "Forum.id",
                target_label: "Forum",
                rel_type: "CONTAINER_OF",
                direction: FkDirection::Incoming,
            },
            ForeignKey {
                column: "place",
                target_label: "Place",
                rel_type: "IS_LOCATED_IN",
                direction: FkDirection::Outgoing,
            },
        ],
    },
    NodeFile {
        path: "dynamic/comment_0_0.csv",
        label: "Comment",
        id_column: "id",
        properties: &[
            Property {
                column: "id",
                name: "id",
                coerce: Coerce::Int,
            },
            Property {
                column: "creationDate",
                name: "creationDate",
                coerce: Coerce::Int,
            },
            Property {
                column: "locationIP",
                name: "locationIP",
                coerce: Coerce::Str,
            },
            Property {
                column: "browserUsed",
                name: "browserUsed",
                coerce: Coerce::Str,
            },
            Property {
                column: "content",
                name: "content",
                coerce: Coerce::Str,
            },
            Property {
                column: "length",
                name: "length",
                coerce: Coerce::Int,
            },
        ],
        foreign_keys: &[
            ForeignKey {
                column: "creator",
                target_label: "Person",
                rel_type: "HAS_CREATOR",
                direction: FkDirection::Outgoing,
            },
            ForeignKey {
                column: "place",
                target_label: "Place",
                rel_type: "IS_LOCATED_IN",
                direction: FkDirection::Outgoing,
            },
            ForeignKey {
                column: "replyOfPost",
                target_label: "Post",
                rel_type: "REPLY_OF",
                direction: FkDirection::Outgoing,
            },
            ForeignKey {
                column: "replyOfComment",
                target_label: "Comment",
                rel_type: "REPLY_OF",
                direction: FkDirection::Outgoing,
            },
        ],
    },
];

pub const EDGE_FILES: &[EdgeFile] = &[
    EdgeFile {
        path: "dynamic/person_knows_person_0_0.csv",
        rel_type: "KNOWS",
        src: ("Person.id", "Person"),
        dst: ("Person.id", "Person"),
        properties: &[Property {
            column: "creationDate",
            name: "creationDate",
            coerce: Coerce::Int,
        }],
        undirected: true,
    },
    EdgeFile {
        path: "dynamic/forum_hasMember_person_0_0.csv",
        rel_type: "HAS_MEMBER",
        src: ("Forum.id", "Forum"),
        dst: ("Person.id", "Person"),
        properties: &[Property {
            column: "joinDate",
            name: "joinDate",
            coerce: Coerce::Int,
        }],
        undirected: false,
    },
    EdgeFile {
        path: "dynamic/forum_hasTag_tag_0_0.csv",
        rel_type: "HAS_TAG",
        src: ("Forum.id", "Forum"),
        dst: ("Tag.id", "Tag"),
        properties: &[],
        undirected: false,
    },
    EdgeFile {
        path: "dynamic/post_hasTag_tag_0_0.csv",
        rel_type: "HAS_TAG",
        src: ("Post.id", "Post"),
        dst: ("Tag.id", "Tag"),
        properties: &[],
        undirected: false,
    },
    EdgeFile {
        path: "dynamic/comment_hasTag_tag_0_0.csv",
        rel_type: "HAS_TAG",
        src: ("Comment.id", "Comment"),
        dst: ("Tag.id", "Tag"),
        properties: &[],
        undirected: false,
    },
    EdgeFile {
        path: "dynamic/person_hasInterest_tag_0_0.csv",
        rel_type: "HAS_INTEREST",
        src: ("Person.id", "Person"),
        dst: ("Tag.id", "Tag"),
        properties: &[],
        undirected: false,
    },
    EdgeFile {
        path: "dynamic/person_likes_post_0_0.csv",
        rel_type: "LIKES",
        src: ("Person.id", "Person"),
        dst: ("Post.id", "Post"),
        properties: &[Property {
            column: "creationDate",
            name: "creationDate",
            coerce: Coerce::Int,
        }],
        undirected: false,
    },
    EdgeFile {
        path: "dynamic/person_likes_comment_0_0.csv",
        rel_type: "LIKES",
        src: ("Person.id", "Person"),
        dst: ("Comment.id", "Comment"),
        properties: &[Property {
            column: "creationDate",
            name: "creationDate",
            coerce: Coerce::Int,
        }],
        undirected: false,
    },
    EdgeFile {
        path: "dynamic/person_studyAt_organisation_0_0.csv",
        rel_type: "STUDY_AT",
        src: ("Person.id", "Person"),
        dst: ("Organisation.id", "Organisation"),
        properties: &[Property {
            column: "classYear",
            name: "classYear",
            coerce: Coerce::Int,
        }],
        undirected: false,
    },
    EdgeFile {
        path: "dynamic/person_workAt_organisation_0_0.csv",
        rel_type: "WORK_AT",
        src: ("Person.id", "Person"),
        dst: ("Organisation.id", "Organisation"),
        properties: &[Property {
            column: "workFrom",
            name: "workFrom",
            coerce: Coerce::Int,
        }],
        undirected: false,
    },
];

/// Record counts published in `benchmarks/ldbc-snb/README.md` for SF0.1,
/// used as a dataset-integrity check before anything is written to the
/// database: a truncated or half-extracted download is caught in seconds
/// instead of showing up as a mysterious count mismatch after a full load.
pub const SF0_1_NODE_COUNTS: &[(&str, usize)] = &[
    ("Person", 1_528),
    ("Forum", 13_750),
    ("Post", 135_701),
    ("Comment", 151_043),
    ("Place", 1_460),
    ("Organisation", 7_955),
    ("Tag", 16_080),
    ("TagClass", 71),
];

/// Row counts of the ten explicit edge files at SF0.1 (README table). Keyed by
/// file path because two files share the `HAS_TAG` type.
pub const SF0_1_EDGE_FILE_ROWS: &[(&str, usize)] = &[
    ("dynamic/person_knows_person_0_0.csv", 14_073),
    ("dynamic/forum_hasMember_person_0_0.csv", 123_268),
    ("dynamic/forum_hasTag_tag_0_0.csv", 47_697),
    ("dynamic/post_hasTag_tag_0_0.csv", 51_118),
    ("dynamic/comment_hasTag_tag_0_0.csv", 191_303),
    ("dynamic/person_hasInterest_tag_0_0.csv", 35_475),
    ("dynamic/person_likes_post_0_0.csv", 47_215),
    ("dynamic/person_likes_comment_0_0.csv", 62_225),
    ("dynamic/person_studyAt_organisation_0_0.csv", 1_209),
    ("dynamic/person_workAt_organisation_0_0.csv", 3_313),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_node_file_carries_its_id_as_a_property() {
        // The LDBC id is what every edge file and every query parameter
        // refers to, so it must be stored, not just used for correlation.
        for file in NODE_FILES {
            assert!(
                file.properties
                    .iter()
                    .any(|p| p.column == file.id_column && p.coerce == Coerce::Int),
                "{} must store its `{}` column as an integer property",
                file.path,
                file.id_column
            );
        }
    }

    #[test]
    fn foreign_key_columns_are_never_also_properties() {
        // A column that becomes an edge must not ALSO be stored as a scalar
        // property: it would duplicate the relationship as dead data and
        // desync if one of the two passes changed.
        for file in NODE_FILES {
            for fk in file.foreign_keys {
                assert!(
                    !file.properties.iter().any(|p| p.column == fk.column),
                    "{}: `{}` is both a foreign key and a property",
                    file.path,
                    fk.column
                );
            }
        }
    }

    #[test]
    fn the_merge_foreign_table_is_complete() {
        // 13 foreign-key COLUMNS across the node files. The README's table has
        // 12 rows because `replyOfPost` and `replyOfComment` share one row
        // (mutually exclusive, same `REPLY_OF` type) — they are still two
        // distinct columns to read. Losing one silently produces a graph
        // missing an entire edge type.
        let columns: usize = NODE_FILES.iter().map(|f| f.foreign_keys.len()).sum();
        assert_eq!(columns, 13, "expected 13 merge-foreign FK columns");

        let mut types: Vec<&str> = NODE_FILES
            .iter()
            .flat_map(|f| f.foreign_keys)
            .map(|fk| fk.rel_type)
            .collect();
        types.sort_unstable();
        types.dedup();
        assert_eq!(
            types,
            vec![
                "CONTAINER_OF",
                "HAS_CREATOR",
                "HAS_MODERATOR",
                "HAS_TYPE",
                "IS_LOCATED_IN",
                "IS_PART_OF",
                "IS_SUBCLASS_OF",
                "REPLY_OF",
            ]
        );
    }

    #[test]
    fn the_two_sources_cover_all_fifteen_snb_relationship_types() {
        // The SNB Interactive schema has exactly 15 relationship types. Any
        // gap here means a whole class of query cannot traverse.
        let mut types: Vec<&str> = NODE_FILES
            .iter()
            .flat_map(|f| f.foreign_keys)
            .map(|fk| fk.rel_type)
            .chain(EDGE_FILES.iter().map(|f| f.rel_type))
            .collect();
        types.sort_unstable();
        types.dedup();
        assert_eq!(
            types,
            vec![
                "CONTAINER_OF",
                "HAS_CREATOR",
                "HAS_INTEREST",
                "HAS_MEMBER",
                "HAS_MODERATOR",
                "HAS_TAG",
                "HAS_TYPE",
                "IS_LOCATED_IN",
                "IS_PART_OF",
                "IS_SUBCLASS_OF",
                "KNOWS",
                "LIKES",
                "REPLY_OF",
                "STUDY_AT",
                "WORK_AT",
            ]
        );
    }

    #[test]
    fn container_of_is_the_only_reversed_foreign_key() {
        // `CONTAINER_OF` is Forum -> Post while the FK sits on the Post row;
        // every other folded edge points away from its own row.
        let reversed: Vec<&str> = NODE_FILES
            .iter()
            .flat_map(|f| f.foreign_keys)
            .filter(|fk| fk.direction == FkDirection::Incoming)
            .map(|fk| fk.rel_type)
            .collect();
        assert_eq!(reversed, vec!["CONTAINER_OF"]);
    }

    #[test]
    fn only_knows_is_undirected() {
        let undirected: Vec<&str> = EDGE_FILES
            .iter()
            .filter(|e| e.undirected)
            .map(|e| e.rel_type)
            .collect();
        assert_eq!(
            undirected,
            vec!["KNOWS"],
            "only person_knows_person is stored once per pair"
        );
    }

    #[test]
    fn node_labels_match_the_expected_count_table() {
        let mut labels: Vec<&str> = NODE_FILES.iter().map(|f| f.label).collect();
        labels.sort_unstable();
        let mut expected: Vec<&str> = SF0_1_NODE_COUNTS.iter().map(|(l, _)| *l).collect();
        expected.sort_unstable();
        assert_eq!(labels, expected);
    }

    #[test]
    fn edge_file_paths_match_the_expected_row_table() {
        let mut paths: Vec<&str> = EDGE_FILES.iter().map(|f| f.path).collect();
        paths.sort_unstable();
        let mut expected: Vec<&str> = SF0_1_EDGE_FILE_ROWS.iter().map(|(p, _)| *p).collect();
        expected.sort_unstable();
        assert_eq!(paths, expected);
    }
}
