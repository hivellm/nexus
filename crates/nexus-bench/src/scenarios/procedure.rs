//! `procedure.*` seed scenarios — `CALL db.*` / `CALL dbms.*`.

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
        ScenarioBuilder::new(
            "procedure.db_labels",
            "db.labels procedure",
            DatasetKind::Tiny,
            "CALL db.labels() YIELD label RETURN count(label) AS c",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "procedure.db_relationship_types",
            "db.relationshipTypes procedure",
            DatasetKind::Tiny,
            "CALL db.relationshipTypes() YIELD relationshipType RETURN count(relationshipType) AS c",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "procedure.db_property_keys",
            "db.propertyKeys procedure",
            DatasetKind::Tiny,
            "CALL db.propertyKeys() YIELD propertyKey RETURN count(propertyKey) AS c",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "procedure.db_indexes",
            "db.indexes procedure — catalogue of indexes",
            DatasetKind::Tiny,
            "CALL db.indexes() YIELD * RETURN count(*) AS c",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "procedure.dbms_components",
            "dbms.components() — server version metadata",
            DatasetKind::Tiny,
            "CALL dbms.components() YIELD name RETURN count(name) AS c",
        )
        .expected_rows(1)
        .build(),
    ]
}
