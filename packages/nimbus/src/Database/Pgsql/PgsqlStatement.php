<?php

declare(strict_types=1);

namespace Nimbus\Database\Pgsql;

use Arcano\Database\Driver\PDOStatementInterface;
use PgSql\Result;

/**
 * A PDO-shaped statement over PgsqlConnection. Integer, float and boolean columns
 * come back as PHP scalars, as pdo_pgsql returns them.
 *
 * @internal
 */
final class PgsqlStatement implements PDOStatementInterface
{
    private const array INTEGER = [20 => true, 21 => true, 23 => true, 26 => true];

    private const array FLOAT = [700 => true, 701 => true];

    private const int BOOLEAN = 16;

    /** @var array<int|string, mixed> */
    private array $bound = [];

    /** @var list<array<string, mixed>> */
    private array $rows = [];

    /** @var list<string> */
    private array $columns = [];

    private int $cursor = 0;

    private int $affected = 0;

    private int $fetchMode = \PDO::FETCH_BOTH;

    public function __construct(
        private readonly PgsqlConnection $connection,
        public readonly string $queryString,
    ) {
    }

    /** @param array<mixed>|null $params */
    public function execute(?array $params = null): bool
    {
        $values = [];
        foreach ($params ?? $this->bound as $value) {
            $values[] = match (true) {
                $value === null => null,
                is_bool($value) => $value ? 't' : 'f',
                is_scalar($value), $value instanceof \Stringable => (string) $value,
                default => throw new \InvalidArgumentException('Postgres parameters must be scalar or null.'),
            };
        }
        $result = $this->connection->run($this->queryString, $values);
        $this->read($result);
        return true;
    }

    public function fetch(int $mode = \PDO::FETCH_DEFAULT, int $cursorOrientation = \PDO::FETCH_ORI_NEXT, int $cursorOffset = 0): mixed
    {
        $row = $this->rows[$this->cursor] ?? null;
        if ($row === null) {
            return false;
        }
        ++$this->cursor;
        return $this->shape($row, $mode === \PDO::FETCH_DEFAULT ? $this->fetchMode : $mode);
    }

    public function bindParam(int|string $param, mixed &$var, int $type = \PDO::PARAM_STR, ?int $maxLength = null, mixed $driverOptions = null): bool
    {
        $this->bound[$param] = &$var;
        return true;
    }

    public function bindColumn(int|string $column, mixed &$var, int $type = \PDO::PARAM_STR, ?int $maxLength = null, mixed $driverOptions = null): bool
    {
        throw new \LogicException('bindColumn() is not supported by the async Postgres driver.');
    }

    public function bindValue(int|string $param, mixed $value, int $type = \PDO::PARAM_STR): bool
    {
        $this->bound[$param] = $type === \PDO::PARAM_BOOL && $value !== null ? (bool) $value : $value;
        return true;
    }

    public function rowCount(): int
    {
        return $this->affected;
    }

    public function fetchColumn(int $column = 0): mixed
    {
        $row = $this->rows[$this->cursor] ?? null;
        if ($row === null) {
            return false;
        }
        ++$this->cursor;
        return array_values($row)[$column] ?? null;
    }

    /** @return array<mixed> */
    public function fetchAll(int $mode = \PDO::FETCH_DEFAULT, mixed ...$args): array
    {
        $mode = $mode === \PDO::FETCH_DEFAULT ? $this->fetchMode : $mode;
        $rows = array_slice($this->rows, $this->cursor);
        $this->cursor = count($this->rows);
        if ($mode === \PDO::FETCH_COLUMN) {
            $column = is_int($args[0] ?? null) ? $args[0] : 0;
            return array_map(static fn (array $row): mixed => array_values($row)[$column] ?? null, $rows);
        }
        return array_map(fn (array $row): mixed => $this->shape($row, $mode), $rows);
    }

    /** @param array<mixed> $constructorArgs */
    public function fetchObject(?string $class = \stdClass::class, array $constructorArgs = []): object|false
    {
        $row = $this->fetch(\PDO::FETCH_ASSOC);
        if (!is_array($row)) {
            return false;
        }
        $object = new ($class ?? \stdClass::class)(...$constructorArgs);
        foreach ($row as $name => $value) {
            $object->{$name} = $value;
        }
        return $object;
    }

    public function errorCode(): ?string
    {
        return null;
    }

    /** @return array{string, int, string} */
    public function errorInfo(): array
    {
        return ['00000', 0, ''];
    }

    public function setAttribute(int $attribute, mixed $value): bool
    {
        return false;
    }

    public function getAttribute(int $name): mixed
    {
        return null;
    }

    public function columnCount(): int
    {
        return count($this->columns);
    }

    /** @return array{name: string}|false */
    public function getColumnMeta(int $column): array|false
    {
        $name = $this->columns[$column] ?? null;
        return $name === null ? false : ['name' => $name];
    }

    public function setFetchMode(mixed $mode, mixed $className = null, mixed ...$params): bool
    {
        if (!is_int($mode)) {
            throw new \InvalidArgumentException('The fetch mode must be a PDO::FETCH_* constant.');
        }
        $this->fetchMode = $mode;
        return true;
    }

    public function nextRowset(): bool
    {
        return false;
    }

    public function closeCursor(): bool
    {
        $this->rows = [];
        $this->cursor = 0;
        return true;
    }

    public function debugDumpParams(): ?bool
    {
        return null;
    }

    /** @return \Generator<int, mixed> */
    public function getIterator(): \Generator
    {
        while (($row = $this->fetch()) !== false) {
            yield $row;
        }
    }

    private function read(Result $result): void
    {
        $this->cursor = 0;
        $this->affected = pg_affected_rows($result);
        $count = pg_num_fields($result);
        $this->columns = [];
        $casts = [];
        for ($i = 0; $i < $count; ++$i) {
            $this->columns[] = pg_field_name($result, $i);
            $oid = pg_field_type_oid($result, $i);
            $casts[$i] = match (true) {
                isset(self::INTEGER[$oid]) => 1,
                isset(self::FLOAT[$oid]) => 2,
                $oid === self::BOOLEAN => 3,
                default => 0,
            };
        }
        $rows = $count === 0 ? [] : pg_fetch_all($result, PGSQL_NUM);
        pg_free_result($result);
        $this->rows = [];
        foreach ($rows as $row) {
            $assoc = [];
            foreach ($row as $i => $value) {
                if ($value !== null && $casts[$i] !== 0) {
                    $value = match ($casts[$i]) {
                        1 => (int) $value,
                        2 => (float) $value,
                        default => $value === 't',
                    };
                }
                $assoc[$this->columns[$i]] = $value;
            }
            $this->rows[] = $assoc;
        }
    }

    /** @param array<string, mixed> $row */
    private function shape(array $row, int $mode): mixed
    {
        return match ($mode) {
            \PDO::FETCH_ASSOC => $row,
            \PDO::FETCH_NUM => array_values($row),
            \PDO::FETCH_OBJ => (object) $row,
            \PDO::FETCH_COLUMN => array_values($row)[0] ?? null,
            default => $row + array_values($row),
        };
    }
}
