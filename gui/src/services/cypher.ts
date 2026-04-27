/**
 * Cypher source-prep helpers shared by the editor's Run path.
 *
 * Nexus' parser refuses queries whose first non-blank token is a
 * comment (`Query must contain at least one clause`). The GUI
 * keeps the user's source intact in the editor and only sanitises
 * what we send over the wire — so comments stay in place for the
 * author, but the executor sees the clauses.
 */

const LINE_COMMENT_RE = /\/\/.*?$|--.*?$/gm;
const BLOCK_COMMENT_RE = /\/\*[\s\S]*?\*\//g;

export function sanitizeCypher(src: string): string {
  return src
    .replace(BLOCK_COMMENT_RE, '')
    .replace(LINE_COMMENT_RE, '')
    .replace(/\r/g, '')
    .split('\n')
    .map((l) => l.trimEnd())
    .filter((l, i, arr) => {
      // Drop leading blank lines so the first non-blank line carries
      // a clause; preserve interior blanks because the parser is
      // whitespace-tolerant once it has seen a clause.
      if (l.trim().length > 0) return true;
      const seenClause = arr.slice(0, i).some((p) => p.trim().length > 0);
      return seenClause;
    })
    .join('\n')
    .replace(/;\s*$/g, '')
    .trim();
}
