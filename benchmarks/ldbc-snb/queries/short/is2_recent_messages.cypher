// LDBC SNB Interactive — IS2 (a person's 10 most recent messages, with the
// original post of each and its author). Params: personId
MATCH (:Person {id: $personId})<-[:HAS_CREATOR]-(message:Message)
WITH message, message.id AS messageId, message.creationDate AS messageCreationDate
ORDER BY messageCreationDate DESC, messageId DESC
LIMIT 10
MATCH (message)-[:REPLY_OF*0..]->(post:Post)-[:HAS_CREATOR]->(person:Person)
RETURN
  messageId,
  coalesce(message.content, message.imageFile) AS messageContent,
  messageCreationDate,
  post.id AS originalPostId,
  person.id AS originalPostAuthorId,
  person.firstName AS originalPostAuthorFirstName,
  person.lastName AS originalPostAuthorLastName
ORDER BY messageCreationDate DESC, messageId DESC
