"""Tests for `scripts/migration/from_kuzu.py`.

Run with `pytest tests/migration/`. The module is imported by path
because `scripts/` is not on `sys.path` by default and we don't want
to ship a `pyproject.toml` for the migration helper.
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = REPO_ROOT / "scripts" / "migration" / "from_kuzu.py"


def _load_module():
    spec = importlib.util.spec_from_file_location("from_kuzu", SCRIPT_PATH)
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    sys.modules["from_kuzu"] = mod
    spec.loader.exec_module(mod)
    return mod


@pytest.fixture(scope="module")
def fk():
    return _load_module()


def test_node_spec_parse_round_trips(fk):
    spec = fk.NodeSpec.parse("Person:/tmp/person.csv")
    assert spec.label == "Person"
    assert spec.csv_path == Path("/tmp/person.csv")


def test_node_spec_rejects_malformed(fk):
    with pytest.raises(ValueError):
        fk.NodeSpec.parse("Person")
    with pytest.raises(ValueError):
        fk.NodeSpec.parse(":/tmp/person.csv")
    with pytest.raises(ValueError):
        fk.NodeSpec.parse("Person:")


def test_rel_spec_parse_round_trips(fk):
    spec = fk.RelSpec.parse("Person-KNOWS-Person:/tmp/knows.csv")
    assert spec.src_label == "Person"
    assert spec.rel_type == "KNOWS"
    assert spec.dst_label == "Person"
    assert spec.csv_path == Path("/tmp/knows.csv")


def test_rel_spec_rejects_missing_parts(fk):
    with pytest.raises(ValueError):
        fk.RelSpec.parse("Person-KNOWS:/tmp/knows.csv")
    with pytest.raises(ValueError):
        fk.RelSpec.parse("Person-KNOWS-Person")


def test_emit_load_csv_node_uses_merge(fk):
    spec = fk.NodeSpec(label="Person", csv_path=Path("kuzu/person.csv"))
    cypher = fk.emit_load_csv_node(spec, id_property="id")
    assert "MERGE (n:Person { id: row.id })" in cypher
    assert "LOAD CSV WITH HEADERS FROM 'file:///kuzu/person.csv'" in cypher
    assert "apoc.map.removeKey(row, 'id')" in cypher


def test_emit_load_csv_rel_drops_from_to(fk):
    spec = fk.RelSpec(
        src_label="Person",
        rel_type="KNOWS",
        dst_label="Person",
        csv_path=Path("kuzu/knows.csv"),
    )
    cypher = fk.emit_load_csv_rel(spec, id_property="id")
    assert "MERGE (a)-[r:KNOWS]->(b)" in cypher
    assert "row.from" in cypher and "row.to" in cypher
    # Property assignment should remove from/to so they don't reappear as relationship props.
    assert "removeKey(apoc.map.removeKey(row, 'from'), 'to')" in cypher


def test_write_load_csv_driver_emits_indexes_and_order(fk, tmp_path):
    nodes = [
        fk.NodeSpec(label="Person", csv_path=tmp_path / "person.csv"),
        fk.NodeSpec(label="Movie", csv_path=tmp_path / "movie.csv"),
    ]
    rels = [
        fk.RelSpec(
            src_label="Person",
            rel_type="ACTED_IN",
            dst_label="Movie",
            csv_path=tmp_path / "acted_in.csv",
        )
    ]
    target = fk.write_load_csv_driver(tmp_path / "out", nodes, rels, "id")
    assert target.exists()
    text = target.read_text(encoding="utf-8")
    # Nodes come before indexes; indexes come before relationships.
    person_idx = text.index("MERGE (n:Person")
    movie_idx = text.index("MERGE (n:Movie")
    index_idx = text.index("CREATE INDEX")
    rel_idx = text.index("MERGE (a)-[r:ACTED_IN]")
    assert person_idx < index_idx
    assert movie_idx < index_idx
    assert index_idx < rel_idx
    # Both labels get an id index.
    assert "FOR (n:Person) ON (n.id)" in text
    assert "FOR (n:Movie) ON (n.id)" in text


def _write_csv(path: Path, header: list[str], rows: list[list[str]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="") as f:
        f.write(",".join(header) + "\n")
        for row in rows:
            f.write(",".join(row) + "\n")


def test_stream_node_rows_yields_label_and_props(fk, tmp_path):
    csv_path = tmp_path / "person.csv"
    _write_csv(
        csv_path,
        ["id", "name", "age"],
        [["1", "Alice", "30"], ["2", "Bob", "29"]],
    )
    spec = fk.NodeSpec(label="Person", csv_path=csv_path)
    rows = list(fk.stream_node_rows(spec))
    assert rows == [
        (["Person"], {"id": "1", "name": "Alice", "age": "30"}),
        (["Person"], {"id": "2", "name": "Bob", "age": "29"}),
    ]


def test_stream_rel_rows_pops_from_to(fk, tmp_path):
    csv_path = tmp_path / "knows.csv"
    _write_csv(
        csv_path,
        ["from", "to", "since"],
        [["1", "2", "2020-01-01"], ["1", "3", "2021-06-15"]],
    )
    spec = fk.RelSpec(
        src_label="Person",
        rel_type="KNOWS",
        dst_label="Person",
        csv_path=csv_path,
    )
    rows = list(fk.stream_rel_rows(spec))
    assert rows == [
        ("1", "2", "KNOWS", {"since": "2020-01-01"}),
        ("1", "3", "KNOWS", {"since": "2021-06-15"}),
    ]


def test_stream_rel_rows_rejects_missing_columns(fk, tmp_path):
    csv_path = tmp_path / "bad.csv"
    _write_csv(csv_path, ["a", "b"], [["1", "2"]])
    spec = fk.RelSpec(
        src_label="X",
        rel_type="Y",
        dst_label="Z",
        csv_path=csv_path,
    )
    with pytest.raises(ValueError):
        list(fk.stream_rel_rows(spec))


def test_translate_cypher_shortest_path(fk):
    src = (
        "MATCH path = (a:Person {name:'Alice'})"
        "-[*SHORTEST 1..6]-(b:Person {name:'Bob'}) RETURN path"
    )
    out, notes = fk.translate_cypher(src)
    assert "[*1..6]" in out
    assert "[*SHORTEST" not in out
    assert any("shortestPath" in n for n in notes)


def test_translate_cypher_hnsw_create(fk):
    src = (
        "CALL CREATE_HNSW_INDEX('Document', 'idx', 'embedding', "
        "mu := 32, efc := 400);"
    )
    out, notes = fk.translate_cypher(src)
    assert "db.knn.create" in out
    assert "M: 32" in out
    assert "efConstruction: 400" in out
    assert any("CREATE_HNSW_INDEX" in n for n in notes)


def test_translate_cypher_hnsw_query_flips_score_note(fk):
    src = "CALL QUERY_VECTOR_INDEX('Document', 'idx', $vec, 10) YIELD node, distance"
    out, notes = fk.translate_cypher(src)
    assert "db.knn.search" in out
    assert "YIELD node, score" in out
    assert any("similarity" in n.lower() for n in notes)


def test_translate_cypher_fts(fk):
    src = (
        "CALL CREATE_FTS_INDEX('Document', 'fts', ['title','body']);"
        "\nCALL QUERY_FTS_INDEX('Document', 'fts', 'graph databases')"
    )
    out, notes = fk.translate_cypher(src)
    assert "db.index.fulltext.createNodeIndex" in out
    assert "db.index.fulltext.queryNodes" in out
    assert any("CREATE_FTS_INDEX" in n for n in notes)
    assert any("QUERY_FTS_INDEX" in n for n in notes)


def test_translate_cypher_passes_through_unfamiliar(fk):
    src = "MATCH (n:Person) WHERE n.age > 30 RETURN n.name"
    out, notes = fk.translate_cypher(src)
    assert out.strip() == src.strip()
    assert notes == []


def test_count_csv_rows(fk, tmp_path):
    csv_path = tmp_path / "x.csv"
    _write_csv(csv_path, ["a"], [["1"], ["2"], ["3"]])
    assert fk._count_csv_rows(csv_path) == 3


def test_main_load_csv_writes_driver(fk, tmp_path):
    person = tmp_path / "person.csv"
    knows = tmp_path / "knows.csv"
    _write_csv(person, ["id", "name"], [["1", "Alice"], ["2", "Bob"]])
    _write_csv(knows, ["from", "to", "since"], [["1", "2", "2020-01-01"]])
    out = tmp_path / "migrated"
    rc = fk.main(
        [
            "load-csv",
            "--node",
            f"Person:{person}",
            "--rel",
            f"Person-KNOWS-Person:{knows}",
            "--out-dir",
            str(out),
        ]
    )
    assert rc == 0
    assert (out / "load.cypher").exists()


def test_main_rewrite_cypher_round_trip(fk, tmp_path, capsys):
    src = tmp_path / "in.cypher"
    src.write_text(
        "MATCH p = (a)-[*SHORTEST 1..3]-(b) RETURN p\n",
        encoding="utf-8",
    )
    rc = fk.main(["rewrite-cypher", str(src)])
    assert rc == 0
    captured = capsys.readouterr()
    assert "[*1..3]" in captured.out


def test_main_rewrite_cypher_to_file(fk, tmp_path):
    src = tmp_path / "in.cypher"
    dst = tmp_path / "out.cypher"
    src.write_text(
        "CALL CREATE_FTS_INDEX('Document', 'idx', ['title']);\n",
        encoding="utf-8",
    )
    rc = fk.main(["rewrite-cypher", str(src), "--output", str(dst)])
    assert rc == 0
    out_text = dst.read_text(encoding="utf-8")
    assert "TRANSLATOR-NOTE" in out_text
    assert "db.index.fulltext.createNodeIndex" in out_text
