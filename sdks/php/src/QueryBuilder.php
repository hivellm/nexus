<?php

declare(strict_types=1);

namespace Nexus\SDK;

/**
 * Fluent API for constructing Cypher queries.
 */
class QueryBuilder
{
    /** @var string[] */
    private array $matchClauses = [];

    /** @var string[] */
    private array $whereClauses = [];

    /** @var string[] */
    private array $createClauses = [];

    /** @var string[] */
    private array $setClauses = [];

    /** @var string[] */
    private array $deleteClauses = [];

    /** @var string[] */
    private array $returnClauses = [];

    /** @var string[] */
    private array $orderByClauses = [];

    private ?int $skipValue = null;
    private ?int $limitValue = null;

    /** @var array<string, mixed> */
    private array $parameters = [];

    /**
     * Create a new QueryBuilder instance.
     */
    public static function create(): self
    {
        return new self();
    }

    /**
     * Add a MATCH clause to the query.
     */
    public function match(string $pattern): self
    {
        $this->matchClauses[] = $pattern;
        return $this;
    }

    /**
     * Add an OPTIONAL MATCH clause to the query.
     */
    public function optionalMatch(string $pattern): self
    {
        $this->matchClauses[] = "OPTIONAL MATCH {$pattern}";
        return $this;
    }

    /**
     * Add a WHERE clause to the query.
     */
    public function where(string $condition): self
    {
        $this->whereClauses[] = $condition;
        return $this;
    }

    /**
     * Add an AND condition to the WHERE clause.
     */
    public function and(string $condition): self
    {
        if (count($this->whereClauses) > 0) {
            $lastIndex = count($this->whereClauses) - 1;
            $this->whereClauses[$lastIndex] .= " AND {$condition}";
        } else {
            $this->whereClauses[] = $condition;
        }
        return $this;
    }

    /**
     * Add an OR condition to the WHERE clause.
     */
    public function or(string $condition): self
    {
        if (count($this->whereClauses) > 0) {
            $lastIndex = count($this->whereClauses) - 1;
            $this->whereClauses[$lastIndex] .= " OR {$condition}";
        } else {
            $this->whereClauses[] = $condition;
        }
        return $this;
    }

    /**
     * Add a CREATE clause to the query.
     */
    public function createPattern(string $pattern): self
    {
        $this->createClauses[] = $pattern;
        return $this;
    }

    /**
     * Add a MERGE clause to the query.
     */
    public function merge(string $pattern): self
    {
        $this->createClauses[] = "MERGE {$pattern}";
        return $this;
    }

    /**
     * Add a SET clause to the query.
     */
    public function set(string $assignment): self
    {
        $this->setClauses[] = $assignment;
        return $this;
    }

    /**
     * Add a DELETE clause to the query.
     */
    public function delete(string $items): self
    {
        $this->deleteClauses[] = $items;
        return $this;
    }

    /**
     * Add a DETACH DELETE clause to the query.
     */
    public function detachDelete(string $items): self
    {
        $this->deleteClauses[] = "DETACH DELETE {$items}";
        return $this;
    }

    /**
     * Add a RETURN clause to the query.
     */
    public function return(string ...$items): self
    {
        $this->returnClauses = array_merge($this->returnClauses, $items);
        return $this;
    }

    /**
     * Add a RETURN DISTINCT clause to the query.
     */
    public function returnDistinct(string ...$items): self
    {
        if (count($this->returnClauses) === 0) {
            $this->returnClauses[] = 'DISTINCT ' . implode(', ', $items);
        } else {
            $this->returnClauses = array_merge($this->returnClauses, $items);
        }
        return $this;
    }

    /**
     * Add an ORDER BY clause to the query.
     */
    public function orderBy(string ...$items): self
    {
        $this->orderByClauses = array_merge($this->orderByClauses, $items);
        return $this;
    }

    /**
     * Add an ORDER BY ... DESC clause to the query.
     */
    public function orderByDesc(string $item): self
    {
        $this->orderByClauses[] = "{$item} DESC";
        return $this;
    }

    /**
     * Add a SKIP clause to the query.
     */
    public function skip(int $n): self
    {
        $this->skipValue = $n;
        return $this;
    }

    /**
     * Add a LIMIT clause to the query.
     */
    public function limit(int $n): self
    {
        $this->limitValue = $n;
        return $this;
    }

    /**
     * Add a parameter to the query.
     */
    public function withParam(string $name, mixed $value): self
    {
        $this->parameters[$name] = $value;
        return $this;
    }

    /**
     * Add multiple parameters to the query.
     *
     * @param array<string, mixed> $params
     */
    public function withParams(array $params): self
    {
        foreach ($params as $key => $value) {
            $this->parameters[$key] = $value;
        }
        return $this;
    }

    /**
     * Build the final Cypher query string.
     */
    public function build(): string
    {
        $parts = [];

        // MATCH clauses
        foreach ($this->matchClauses as $match) {
            if (str_starts_with($match, 'OPTIONAL MATCH')) {
                $parts[] = $match;
            } else {
                $parts[] = "MATCH {$match}";
            }
        }

        // WHERE clauses
        if (count($this->whereClauses) > 0) {
            $parts[] = 'WHERE ' . implode(' AND ', $this->whereClauses);
        }

        // CREATE/MERGE clauses
        foreach ($this->createClauses as $create) {
            if (str_starts_with($create, 'MERGE')) {
                $parts[] = $create;
            } else {
                $parts[] = "CREATE {$create}";
            }
        }

        // SET clauses
        if (count($this->setClauses) > 0) {
            $parts[] = 'SET ' . implode(', ', $this->setClauses);
        }

        // DELETE clauses
        foreach ($this->deleteClauses as $del) {
            if (str_starts_with($del, 'DETACH DELETE')) {
                $parts[] = $del;
            } else {
                $parts[] = "DELETE {$del}";
            }
        }

        // RETURN clause
        if (count($this->returnClauses) > 0) {
            $parts[] = 'RETURN ' . implode(', ', $this->returnClauses);
        }

        // ORDER BY clause
        if (count($this->orderByClauses) > 0) {
            $parts[] = 'ORDER BY ' . implode(', ', $this->orderByClauses);
        }

        // SKIP clause
        if ($this->skipValue !== null) {
            $parts[] = "SKIP {$this->skipValue}";
        }

        // LIMIT clause
        if ($this->limitValue !== null) {
            $parts[] = "LIMIT {$this->limitValue}";
        }

        return implode(' ', $parts);
    }

    /**
     * Get the parameters for the query.
     *
     * @return array<string, mixed>
     */
    public function getParameters(): array
    {
        return $this->parameters;
    }
}

/**
 * Builder for node patterns in Cypher queries.
 */
class NodePattern
{
    private string $variable;
    /** @var string[] */
    private array $labels = [];
    /** @var array<string, mixed> */
    private array $properties = [];

    public function __construct(string $variable = '')
    {
        $this->variable = $variable;
    }

    /**
     * Create a new NodePattern instance.
     */
    public static function create(string $variable = ''): self
    {
        return new self($variable);
    }

    /**
     * Set the variable name.
     */
    public function variable(string $variable): self
    {
        $this->variable = $variable;
        return $this;
    }

    /**
     * Add a label to the node.
     */
    public function withLabel(string $label): self
    {
        $this->labels[] = $label;
        return $this;
    }

    /**
     * Add multiple labels to the node.
     */
    public function withLabels(string ...$labels): self
    {
        $this->labels = array_merge($this->labels, $labels);
        return $this;
    }

    /**
     * Add a property to the node.
     */
    public function withProperty(string $key, mixed $value): self
    {
        $this->properties[$key] = $value;
        return $this;
    }

    /**
     * Add multiple properties to the node.
     *
     * @param array<string, mixed> $properties
     */
    public function withProperties(array $properties): self
    {
        foreach ($properties as $key => $value) {
            $this->properties[$key] = $value;
        }
        return $this;
    }

    /**
     * Build the node pattern string.
     */
    public function build(): string
    {
        $result = '(' . $this->variable;

        foreach ($this->labels as $label) {
            $result .= ':' . $label;
        }

        if (count($this->properties) > 0) {
            $props = [];
            foreach ($this->properties as $key => $value) {
                $props[] = "{$key}: " . self::formatValue($value);
            }
            $result .= ' {' . implode(', ', $props) . '}';
        }

        $result .= ')';
        return $result;
    }

    /**
     * Convert to string.
     */
    public function __toString(): string
    {
        return $this->build();
    }

    private static function formatValue(mixed $value): string
    {
        if ($value === null) {
            return 'null';
        }
        if (is_bool($value)) {
            return $value ? 'true' : 'false';
        }
        if (is_string($value)) {
            return "'" . str_replace("'", "\\'", $value) . "'";
        }
        return (string) $value;
    }
}

/**
 * Builder for relationship patterns in Cypher queries.
 */
class RelationshipPattern
{
    private string $variable;
    private string $type = '';
    private string $direction = '->'; // default outgoing
    private ?int $minHops = null;
    private ?int $maxHops = null;

    public function __construct(string $variable = '')
    {
        $this->variable = $variable;
    }

    /**
     * Create a new RelationshipPattern instance.
     */
    public static function create(string $variable = ''): self
    {
        return new self($variable);
    }

    /**
     * Set the variable name.
     */
    public function variable(string $variable): self
    {
        $this->variable = $variable;
        return $this;
    }

    /**
     * Set the relationship type.
     */
    public function withType(string $type): self
    {
        $this->type = $type;
        return $this;
    }

    /**
     * Set the direction to outgoing (->).
     */
    public function outgoing(): self
    {
        $this->direction = '->';
        return $this;
    }

    /**
     * Set the direction to incoming (<-).
     */
    public function incoming(): self
    {
        $this->direction = '<-';
        return $this;
    }

    /**
     * Set the relationship to undirected (-).
     */
    public function undirected(): self
    {
        $this->direction = '';
        return $this;
    }

    /**
     * Set variable length path hops.
     */
    public function withHops(int $min, int $max): self
    {
        $this->minHops = $min;
        $this->maxHops = $max;
        return $this;
    }

    /**
     * Set minimum hops for variable length path.
     */
    public function withMinHops(int $min): self
    {
        $this->minHops = $min;
        return $this;
    }

    /**
     * Set maximum hops for variable length path.
     */
    public function withMaxHops(int $max): self
    {
        $this->maxHops = $max;
        return $this;
    }

    /**
     * Build the relationship pattern string.
     */
    public function build(): string
    {
        $result = '';

        // Start arrow
        if ($this->direction === '<-') {
            $result .= '<-[';
        } else {
            $result .= '-[';
        }

        $result .= $this->variable;

        if ($this->type !== '') {
            $result .= ':' . $this->type;
        }

        // Variable length
        if ($this->minHops !== null || $this->maxHops !== null) {
            $result .= '*';
            if ($this->minHops !== null) {
                $result .= $this->minHops;
            }
            $result .= '..';
            if ($this->maxHops !== null) {
                $result .= $this->maxHops;
            }
        }

        $result .= ']-';

        // End arrow
        if ($this->direction === '->') {
            $result .= '>';
        }

        return $result;
    }

    /**
     * Convert to string.
     */
    public function __toString(): string
    {
        return $this->build();
    }
}

/**
 * Helper class for building path patterns.
 */
class PathBuilder
{
    /**
     * Combine patterns into a path.
     */
    public static function path(string ...$patterns): string
    {
        return implode('', $patterns);
    }
}
