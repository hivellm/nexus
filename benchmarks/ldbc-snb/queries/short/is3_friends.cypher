// LDBC SNB Interactive — IS3 (a person's friends and friendship dates).
// Params: personId
MATCH (n:Person {id: $personId})-[r:KNOWS]-(friend:Person)
RETURN
  friend.id AS personId,
  friend.firstName AS firstName,
  friend.lastName AS lastName,
  r.creationDate AS friendshipCreationDate
ORDER BY friendshipCreationDate DESC, personId ASC
