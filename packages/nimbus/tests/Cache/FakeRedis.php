<?php

declare(strict_types=1);

namespace Nimbus\Tests\Cache;

use Nimbus\Redis\Connection;

/** Enough of Redis for the cache store: strings with expiry, NX, INCRBY, DEL/UNLINK, SCAN and the lock release script. */
final class FakeRedis implements Connection
{
    /** @var array<string, string> */
    private array $values = [];

    public function command(array $arguments): mixed
    {
        $command = strtoupper((string) $arguments[0]);
        $key = (string) ($arguments[1] ?? '');
        switch ($command) {
            case 'GET':
                return $this->values[$key] ?? null;
            case 'SET':
                if (in_array('NX', $arguments, true) && isset($this->values[$key])) {
                    return null;
                }
                $this->values[$key] = (string) $arguments[2];
                return 'OK';
            case 'INCRBY':
                $value = (int) ($this->values[$key] ?? 0) + (int) $arguments[2];
                $this->values[$key] = (string) $value;
                return $value;
            case 'DEL':
            case 'UNLINK':
                $deleted = 0;
                foreach (array_slice($arguments, 1) as $name) {
                    if (isset($this->values[(string) $name])) {
                        unset($this->values[(string) $name]);
                        ++$deleted;
                    }
                }
                return $deleted;
            case 'EVALSHA':
            case 'EVAL':
                // Only the compare-and-delete script of RedisLock.
                $lock = (string) $arguments[3];
                if (($this->values[$lock] ?? null) === (string) $arguments[4]) {
                    unset($this->values[$lock]);
                    return 1;
                }
                return 0;
            case 'SCAN':
                $prefix = rtrim((string) $arguments[3], '*');
                $keys = array_values(array_filter(array_map('strval', array_keys($this->values)), static fn (string $name): bool => str_starts_with($name, $prefix)));
                return ['0', $keys];
            default:
                throw new \LogicException("Unsupported command {$command}.");
        }
    }
}
