// LDBC SNB Interactive — IS4 (content and creation date of a message).
// Params: messageId
MATCH (m:Message {id: $messageId})
RETURN
  m.creationDate AS messageCreationDate,
  coalesce(m.content, m.imageFile) AS messageContent
