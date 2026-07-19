// LDBC SNB Interactive — schema preparation.
//
// Run against a FRESH Nexus database BEFORE loading the CSVs. Creating the
// indexes up front means the loader populates them incrementally instead of
// paying for a full rebuild afterwards.
//
//   ./create-schema.sh                    # localhost:15474, database `ldbc`
//   ./create-schema.ps1 -Database ldbc
//
// Label indexes are NOT declared here: Nexus maintains a RoaringBitmap per
// label automatically, so `MATCH (n:Person)` is already an indexed lookup.
// Only property indexes need DDL.
//
// Every statement is `IF NOT EXISTS`, so re-running against an already
// prepared database is a no-op rather than an error.

// --- Primary key lookups -------------------------------------------------
// Every Interactive query enters the graph through one of these. IS1-IS7 and
// IC1-IC14 all start from a Person.id; IS4-IS7 start from a Message id.
// LDBC ids are not Nexus node ids, so these are ordinary property indexes.
CREATE INDEX snb_person_id IF NOT EXISTS FOR (n:Person) ON (n.id);
CREATE INDEX snb_post_id IF NOT EXISTS FOR (n:Post) ON (n.id);
CREATE INDEX snb_comment_id IF NOT EXISTS FOR (n:Comment) ON (n.id);
CREATE INDEX snb_forum_id IF NOT EXISTS FOR (n:Forum) ON (n.id);
CREATE INDEX snb_organisation_id IF NOT EXISTS FOR (n:Organisation) ON (n.id);
CREATE INDEX snb_place_id IF NOT EXISTS FOR (n:Place) ON (n.id);
CREATE INDEX snb_tag_id IF NOT EXISTS FOR (n:Tag) ON (n.id);
CREATE INDEX snb_tagclass_id IF NOT EXISTS FOR (n:TagClass) ON (n.id);

// --- Name lookups --------------------------------------------------------
// IC3 selects two countries by name, IC11 one; IC6 and IC12 enter through a
// Tag / TagClass name rather than an id.
CREATE INDEX snb_place_name IF NOT EXISTS FOR (n:Place) ON (n.name);
CREATE INDEX snb_tag_name IF NOT EXISTS FOR (n:Tag) ON (n.name);
CREATE INDEX snb_tagclass_name IF NOT EXISTS FOR (n:TagClass) ON (n.name);

// --- creationDate ranges and ordering ------------------------------------
// The read mix is dominated by "most recent N" shapes: IC2, IC5, IC8 and IC9
// bound creationDate and then order by it descending. Message creationDate
// carries the traffic; Person and Forum are indexed for the update workload's
// consistency checks.
CREATE INDEX snb_post_creation_date IF NOT EXISTS FOR (n:Post) ON (n.creationDate);
CREATE INDEX snb_comment_creation_date IF NOT EXISTS FOR (n:Comment) ON (n.creationDate);
CREATE INDEX snb_person_creation_date IF NOT EXISTS FOR (n:Person) ON (n.creationDate);
CREATE INDEX snb_forum_creation_date IF NOT EXISTS FOR (n:Forum) ON (n.creationDate);
