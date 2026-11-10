<?php

declare(strict_types=1);

namespace Pam\Shared {
    /**
     * A fixed-size table in shared memory with the API of Swoole\Table.
     *
     * Without a name, the table is shared with processes forked after it is
     * created, like Swoole\Table. With a name, every worker of the PAM cluster
     * that opens the same name and columns gets the same rows; the master owns
     * the memory, so rows survive worker recycling and reloads and are freed
     * when the server stops. Memory is only committed as rows are written.
     *
     * @implements \Iterator<string, array<string, int|float|string>>
     */
    class Table implements \Iterator, \Countable
    {
        public const TYPE_INT = 1;
        public const TYPE_FLOAT = 2;
        public const TYPE_STRING = 3;

        /** @var list<array{0: string, 1: int, 2: int}> */
        private array $definition = [];
        private ?int $handle = null;
        private int $cursor = 0;
        /** @var list<array{0: string, 1: array<string, int|float|string>}> */
        private array $batch = [];
        private int $position = 0;

        public function __construct(
            private readonly int $size,
            float $conflictProportion = 0.2,
            private readonly string $name = '',
            private readonly int $keySize = 64,
        ) {
            unset($conflictProportion);
            if ($size < 1) {
                throw new \ValueError('Table size must be at least 1.');
            }
        }

        public static function available(): bool
        {
            return \function_exists('pam_native_table_open');
        }

        public function column(string $name, int $type, int $size = 0): bool
        {
            if ($this->handle !== null) {
                return false;
            }
            $this->definition[] = [$name, $type, $type === self::TYPE_STRING ? $size : 8];

            return true;
        }

        /**
         * Allocates the table. Named tables are attached lazily on first use so
         * a configured but unused table costs nothing.
         */
        public function create(): bool
        {
            if ($this->name === '') {
                $this->handle();
            }

            return true;
        }

        /** @param array<string, mixed> $values */
        public function set(string $key, array $values): bool
        {
            return \pam_native_table_set($this->handle(), $key, $values);
        }

        /** @return array<string, int|float|string>|int|float|string|false */
        public function get(string $key, ?string $field = null): array|int|float|string|false
        {
            return \pam_native_table_get($this->handle(), $key, $field);
        }

        public function exists(string $key): bool
        {
            return \pam_native_table_exists($this->handle(), $key);
        }

        public function exist(string $key): bool
        {
            return $this->exists($key);
        }

        public function del(string $key): bool
        {
            return \pam_native_table_del($this->handle(), $key);
        }

        public function delete(string $key): bool
        {
            return $this->del($key);
        }

        public function incr(string $key, string $column, int|float $incrby = 1): int|float|false
        {
            return \pam_native_table_incr($this->handle(), $key, $column, $incrby);
        }

        public function decr(string $key, string $column, int|float $decrby = 1): int|float|false
        {
            return \pam_native_table_incr($this->handle(), $key, $column, -$decrby);
        }

        public function count(): int
        {
            return \pam_native_table_count($this->handle());
        }

        public function getSize(): int
        {
            return $this->stats()['size'];
        }

        public function getMemorySize(): int
        {
            return $this->stats()['memory_size'];
        }

        /** @return array{size: int, memory_size: int, key_size: int, recoveries: int} */
        public function stats(): array
        {
            return \pam_native_table_info($this->handle());
        }

        public function destroy(): bool
        {
            if ($this->handle !== null) {
                \pam_native_table_close($this->handle);
                $this->handle = null;
            }

            return true;
        }

        public function rewind(): void
        {
            $this->cursor = 0;
            $this->batch = [];
            $this->position = 0;
            $this->fetch();
        }

        public function valid(): bool
        {
            return isset($this->batch[$this->position]);
        }

        /** @return array<string, int|float|string>|null */
        public function current(): ?array
        {
            return $this->batch[$this->position][1] ?? null;
        }

        public function key(): ?string
        {
            return $this->batch[$this->position][0] ?? null;
        }

        public function next(): void
        {
            $this->position++;
            if (!isset($this->batch[$this->position])) {
                $this->fetch();
            }
        }

        private function fetch(): void
        {
            $this->batch = [];
            $this->position = 0;
            while ($this->cursor >= 0 && $this->batch === []) {
                [$this->cursor, $this->batch] = \pam_native_table_scan($this->handle(), $this->cursor, 128);
            }
        }

        private function handle(): int
        {
            return $this->handle ??= \pam_native_table_open($this->name, $this->size, $this->definition, $this->keySize);
        }

        private function __clone()
        {
        }

        public function __destruct()
        {
            $this->destroy();
        }
    }

    /**
     * Runs serialized work on the cluster's task workers (`--task-workers`).
     */
    final class Tasks
    {
        /** True in an HTTP worker of a cluster started with task workers. */
        public static function available(): bool
        {
            return \function_exists('pam_native_task_available') && \pam_native_task_available();
        }

        /** True inside a task worker. */
        public static function isTaskWorker(): bool
        {
            return getenv('PAM_TASK_WORKER') === '1';
        }

        /**
         * Runs each payload on a task worker concurrently and waits up to
         * $waitMilliseconds. Each result is the task worker's response, or
         * null when the task was still running when the wait ended.
         *
         * @param list<string> $payloads
         * @return list<string|null>
         */
        public static function resolve(array $payloads, int $waitMilliseconds): array
        {
            return $payloads === [] ? [] : \pam_native_task_resolve(array_values($payloads), $waitMilliseconds);
        }

        /** @param list<string> $payloads */
        public static function dispatch(array $payloads): void
        {
            if ($payloads !== []) {
                \pam_native_task_dispatch(array_values($payloads));
            }
        }

        /** Waits for dispatched tasks to be delivered; returns how many were not. */
        public static function flush(int $timeoutMilliseconds = 5000): int
        {
            return \function_exists('pam_native_task_flush') ? \pam_native_task_flush($timeoutMilliseconds) : 0;
        }
    }
}
