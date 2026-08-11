<?php

declare(strict_types=1);

namespace Pam\Database {
    final readonly class PdoResult
    {
        /** @param list<array<string, mixed>> $rows */
        public function __construct(
            public array $rows,
            public int $affectedRows,
        ) {
        }
    }

    /**
     * Executes blocking PDO drivers in isolated PAM processes. This is intended
     * for legacy/slow drivers that cannot participate in Tokio readiness; the
     * calling Fiber suspends while the bounded process pool performs the query.
     */
    final class IsolatedPdoPool
    {
        private const WORKER = <<<'PHP'
$input = json_decode(stream_get_contents(STDIN), true, 64, JSON_THROW_ON_ERROR);
try {
    $pdo = new PDO(
        $input['dsn'],
        $input['username'],
        $input['password'],
        [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION, PDO::ATTR_EMULATE_PREPARES => false],
    );
    $statement = $pdo->prepare($input['sql']);
    $statement->execute($input['parameters']);
    $rows = $statement->columnCount() > 0
        ? $statement->fetchAll(PDO::FETCH_ASSOC)
        : [];
    echo json_encode([
        'ok' => true,
        'rows' => $rows,
        'affectedRows' => $statement->rowCount(),
    ], JSON_THROW_ON_ERROR | JSON_UNESCAPED_UNICODE | JSON_INVALID_UTF8_SUBSTITUTE);
} catch (Throwable $error) {
    echo json_encode(['ok' => false, 'error' => $error->getMessage()], JSON_THROW_ON_ERROR);
    exit(1);
}
PHP;

        private int $active = 0;
        private int $waiting = 0;

        public function __construct(
            private readonly string $dsn,
            private readonly ?string $username = null,
            private readonly ?string $password = null,
            private readonly int $maxWorkers = 4,
            private readonly int $maxQueue = 128,
            private readonly float $timeout = 30.0,
            private readonly int $maxResultBytes = 16 * 1024 * 1024,
        ) {
            if (
                $dsn === '' || str_contains($dsn, "\0") || $maxWorkers < 1 || $maxWorkers > 64
                || $maxQueue < 1 || $timeout <= 0 || $maxResultBytes < 1
            ) {
                throw new \InvalidArgumentException('Isolated PDO pool options are invalid.');
            }
        }

        /** @param array<int|string, scalar|null> $parameters */
        public function query(string $sql, array $parameters = []): PdoResult
        {
            if ($sql === '' || str_contains($sql, "\0")) {
                throw new \InvalidArgumentException('PDO query must be non-empty and contain no NUL bytes.');
            }
            foreach ($parameters as $value) {
                if (!is_scalar($value) && $value !== null) {
                    throw new \InvalidArgumentException('PDO parameters must be scalar or null.');
                }
            }
            $deadline = microtime(true) + $this->timeout;
            if ($this->active >= $this->maxWorkers) {
                if ($this->waiting >= $this->maxQueue) {
                    throw new \RuntimeException('Isolated PDO pool queue is full.');
                }
                ++$this->waiting;
                try {
                    while ($this->active >= $this->maxWorkers) {
                        $remaining = $deadline - microtime(true);
                        if ($remaining <= 0) {
                            throw new \RuntimeException('Timed out waiting for an isolated PDO worker.');
                        }
                        \Pam\Async\delay(min(0.001, $remaining));
                    }
                } finally {
                    --$this->waiting;
                }
            }
            ++$this->active;
            try {
                $payload = json_encode([
                    'dsn' => $this->dsn,
                    'username' => $this->username,
                    'password' => $this->password,
                    'sql' => $sql,
                    'parameters' => $parameters,
                ], JSON_THROW_ON_ERROR | JSON_UNESCAPED_UNICODE);
                $result = \Pam\Process\Command::run(
                    [PHP_BINARY, '-r', self::WORKER],
                    $payload,
                    max(0.001, $deadline - microtime(true)),
                    $this->maxResultBytes,
                );
                $decoded = json_decode($result->stdout, true, 64, JSON_THROW_ON_ERROR);
                if (!is_array($decoded) || ($decoded['ok'] ?? false) !== true) {
                    $message = is_array($decoded) && is_string($decoded['error'] ?? null)
                        ? $decoded['error']
                        : ($result->stderr !== '' ? $result->stderr : 'isolated PDO worker failed');
                    throw new \RuntimeException($message);
                }
                $rows = is_array($decoded['rows'] ?? null) ? array_values($decoded['rows']) : [];
                $affectedRows = is_int($decoded['affectedRows'] ?? null) ? $decoded['affectedRows'] : 0;
                return new PdoResult($rows, $affectedRows);
            } finally {
                --$this->active;
            }
        }

        public function activeWorkers(): int
        {
            return $this->active;
        }

        public function queuedQueries(): int
        {
            return $this->waiting;
        }
    }
}
