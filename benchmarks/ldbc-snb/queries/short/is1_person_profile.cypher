// LDBC SNB Interactive — IS1 (Person profile)
// Params: personId  (from interactive_1_param.txt: personId column)
MATCH (n:Person {id: $personId})-[:IS_LOCATED_IN]->(p:Place)
RETURN
  n.firstName AS firstName,
  n.lastName AS lastName,
  n.birthday AS birthday,
  n.locationIP AS locationIP,
  n.browserUsed AS browserUsed,
  p.id AS cityId,
  n.gender AS gender,
  n.creationDate AS creationDate
