<?php

declare(strict_types=1);

namespace Pam\Octane\Tables;

use Laravel\Octane\Exceptions\ValueTooLargeForColumnException;

/**
 * An Octane table on PAM shared memory, with the API and the column size
 * checks of Octane's SwooleTable.
 *
 * Keys longer than the table's key size (64 bytes, like Swoole) are stored
 * under a collision-free digest instead of being truncated.
 */
class PamTable extends \Pam\Shared\Table
{
    /** @var array<string, array{0: int, 1: int}> */
    protected array $columns = [];

    public function __construct(int $size, string $name = '', private readonly int $maxKeyBytes = 64)
    {
        parent::__construct($size, 1, $name, $maxKeyBytes);
    }

    public function column(string $name, int $type, int $size = 0): bool
    {
        $this->columns[$name] = [$type, $size];

        return parent::column($name, $type, $size);
    }

    /** @param array<string, mixed> $values */
    public function set(string $key, array $values): bool
    {
        foreach ($values as $column => $value) {
            if (!isset($this->columns[$column])) {
                continue;
            }
            [$type, $size] = $this->columns[$column];
            if ($type !== self::TYPE_STRING || !is_scalar($value)) {
                continue;
            }
            $text = (string) $value;
            if (strlen($text) > $size) {
                throw new ValueTooLargeForColumnException(sprintf(
                    'Value [%s...] is too large for [%s] column.',
                    substr($text, 0, 20),
                    $column,
                ));
            }
        }

        return parent::set($this->storageKey($key), $values);
    }

    public function get(string $key, ?string $field = null): array|int|float|string|false
    {
        return parent::get($this->storageKey($key), $field);
    }

    public function exists(string $key): bool
    {
        return parent::exists($this->storageKey($key));
    }

    public function del(string $key): bool
    {
        return parent::del($this->storageKey($key));
    }

    public function incr(string $key, string $column, int|float $incrby = 1): int|float|false
    {
        return parent::incr($this->storageKey($key), $column, $incrby);
    }

    public function decr(string $key, string $column, int|float $decrby = 1): int|float|false
    {
        return parent::decr($this->storageKey($key), $column, $decrby);
    }

    private function storageKey(string $key): string
    {
        if (strlen($key) <= $this->maxKeyBytes) {
            return $key;
        }

        // "#" + 40 hex characters; the prefix keeps keys readable in iteration.
        return substr($key, 0, $this->maxKeyBytes - 41).'#'.sha1($key);
    }
}
