// LDBC SNB Interactive — IS5 (the author of a message). Params: messageId
MATCH (m:Message {id: $messageId})-[:HAS_CREATOR]->(p:Person)
RETURN
  p.id AS personId,
  p.firstName AS firstName,
  p.lastName AS lastName
