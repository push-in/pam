<?php

declare(strict_types=1);

namespace Nimbus\Database\Pgsql;

use Arcano\Database\Driver\PDOStatementInterface;

/**
 * A PDO-shaped statement over NativePgConnection. Values are bound as parameters of the
 * server-side prepared statement (prepared once per session); integer, float and boolean
 * columns come back as PHP scalars and bytea as a stream, as pdo_pgsql returns them.
 *
 * @internal
 */
final class NativePgStatement implements PDOStatementInterface
{
    private const int NUM = 2;

    /** @var array<int|string, mixed> Values as pam_pg_execute() takes them, by placeholder. */
    private array $bound = [];

    /** @var array<int|string, mixed> bindParam() variables, read at execute(). */
    private array $references = [];

    /** @var array<int|string, int> */
    private array $referenceTypes = [];

    /** @var list<list<mixed>> Rows by column position (the ORM fetches FETCH_NUM). */
    private array $rows = [];

    /** @var list<string> */
    private array $columns = [];

    private int $cursor = 0;

    private int $affected = 0;

    private int $fetchMode = \PDO::FETCH_BOTH;

    private bool $prepared = false;

    public function __construct(
        private readonly int $session,
        public readonly string $queryString,
    ) {
    }

    /** Like pdo_pgsql: the server-side statement lives as long as this object. */
    public function __destruct()
    {
        if ($this->prepared) {
            pam_pg_release($this->session, $this->queryString);
        }
    }

    /**
     * Positional keys may come in any order (bindValue(2) before bindValue(1)); the
     * runtime orders them.
     *
     * @param array<mixed>|null $params
     */
    public function execute(?array $params = null): bool
    {
        if ($params !== null) {
            $values = [];
            foreach ($params as $key => $value) {
                $values[$key] = self::value($value, \PDO::PARAM_STR);
            }
        } else {
            $values = $this->bound;
            foreach ($this->references as $key => $value) {
                $values[$key] = self::value($value, $this->referenceTypes[$key] ?? \PDO::PARAM_STR);
            }
        }
        $this->prepared = true;
        [$this->affected, $this->columns, $this->rows] = pam_pg_execute($this->session, $this->queryString, $values, self::NUM);
        $this->cursor = 0;
        return true;
    }

    /**
     * Replaces every bound value at once (NativePostgresDriver::bindParameters()).
     *
     * @param array<int|string, mixed> $values already converted with value()
     */
    public function bindValues(array $values): void
    {
        $this->bound = $values;
        $this->references = [];
        $this->referenceTypes = [];
    }

    public function fetch(int $mode = \PDO::FETCH_DEFAULT, int $cursorOrientation = \PDO::FETCH_ORI_NEXT, int $cursorOffset = 0): mixed
    {
        $row = $this->rows[$this->cursor] ?? null;
        if ($row === null) {
            return false;
        }
        ++$this->cursor;
        if ($mode === \PDO::FETCH_DEFAULT) {
            $mode = $this->fetchMode;
        }
        return $mode === \PDO::FETCH_NUM ? $row : $this->shape($row, $mode);
    }

    public function bindParam(int|string $param, mixed &$var, int $type = \PDO::PARAM_STR, ?int $maxLength = null, mixed $driverOptions = null): bool
    {
        unset($this->bound[$param]);
        $this->references[$param] = &$var;
        $this->referenceTypes[$param] = $type;
        return true;
    }

    public function bindColumn(int|string $column, mixed &$var, int $type = \PDO::PARAM_STR, ?int $maxLength = null, mixed $driverOptions = null): bool
    {
        throw new \LogicException('bindColumn() is not supported by the pam-pg driver.');
    }

    public function bindValue(int|string $param, mixed $value, int $type = \PDO::PARAM_STR): bool
    {
        unset($this->references[$param], $this->referenceTypes[$param]);
        $this->bound[$param] = self::value($value, $type);
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
        return $row[$column] ?? null;
    }

    /** @return array<mixed> */
    public function fetchAll(int $mode = \PDO::FETCH_DEFAULT, mixed ...$args): array
    {
        $mode = $mode === \PDO::FETCH_DEFAULT ? $this->fetchMode : $mode;
        $rows = $this->cursor === 0 ? $this->rows : array_slice($this->rows, $this->cursor);
        $this->cursor = count($this->rows);
        if ($mode === \PDO::FETCH_NUM) {
            return $rows;
        }
        if ($mode === \PDO::FETCH_COLUMN) {
            $column = is_int($args[0] ?? null) ? $args[0] : 0;
            return array_column($rows, $column);
        }
        if ($mode === \PDO::FETCH_ASSOC) {
            $columns = $this->columns;
            return array_map(static fn (array $row): array => array_combine($columns, $row), $rows);
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

    /**
     * A bound value as pam_pg_execute takes it: scalar, null, Stringable, or [bytes] for binary.
     *
     * @internal
     */
    public static function value(mixed $value, int $type): mixed
    {
        if ($type === \PDO::PARAM_STR && (\is_string($value) || \is_int($value) || $value === null)) {
            return $value;
        }
        if ($value === null || $type === \PDO::PARAM_NULL) {
            return null;
        }
        if ($type === \PDO::PARAM_LOB) {
            $bytes = is_resource($value) ? stream_get_contents($value) : $value;
            if (!is_string($bytes)) {
                throw new \InvalidArgumentException('A PDO::PARAM_LOB value must be a string or a stream.');
            }
            return [$bytes];
        }
        if ($type === \PDO::PARAM_BOOL) {
            return (bool) $value;
        }
        if ($value instanceof \BackedEnum) {
            return $value->value;
        }
        if (is_resource($value)) {
            return stream_get_contents($value);
        }
        return $value;
    }

    /** @param list<mixed> $row */
    private function shape(array $row, int $mode): mixed
    {
        // array_combine() keeps the last of repeated names, as FETCH_ASSOC does.
        return match ($mode) {
            \PDO::FETCH_NUM => $row,
            \PDO::FETCH_ASSOC => array_combine($this->columns, $row),
            \PDO::FETCH_OBJ => (object) array_combine($this->columns, $row),
            \PDO::FETCH_COLUMN => $row[0] ?? null,
            default => array_combine($this->columns, $row) + $row,
        };
    }
}
