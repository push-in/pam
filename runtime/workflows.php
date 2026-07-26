<?php

declare(strict_types=1);

namespace Pam\Workflow {
    enum InstanceState: int
    {
        case Pending = 1;
        case Running = 2;
        case Waiting = 3;
        case Completed = 4;
        case Failed = 5;
        case Compensating = 6;
        case Compensated = 7;
    }

    enum StepState: int
    {
        case Pending = 1;
        case Running = 2;
        case Waiting = 3;
        case Completed = 4;
        case Failed = 5;
        case Compensated = 6;
    }

    final readonly class RetryPolicy
    {
        public function __construct(
            public int $maxAttempts = 3,
            public float $initialDelaySeconds = 1.0,
            public float $multiplier = 2.0,
            public float $maximumDelaySeconds = 300.0,
        ) {
            if ($maxAttempts < 1 || $maxAttempts > 100) {
                throw new \InvalidArgumentException('Workflow maxAttempts must be between 1 and 100.');
            }
            if ($initialDelaySeconds < 0 || $multiplier < 1 || $maximumDelaySeconds < 0) {
                throw new \InvalidArgumentException('Workflow retry timing is invalid.');
            }
        }

        public function delayAfter(int $attempt): float
        {
            return min(
                $this->maximumDelaySeconds,
                $this->initialDelaySeconds * ($this->multiplier ** max(0, $attempt - 1)),
            );
        }
    }

    final readonly class Context
    {
        /**
         * @param array<string, mixed> $input
         * @param array<string, mixed> $results
         */
        public function __construct(
            public string $instanceId,
            public array $input,
            public array $results,
        ) {
        }
    }

    final readonly class Step
    {
        public \Closure $activity;
        public ?\Closure $compensation;

        public function __construct(
            public string $name,
            callable $activity,
            public RetryPolicy $retry = new RetryPolicy(),
            ?callable $compensation = null,
        ) {
            if (preg_match('/^[a-z][a-z0-9._-]{0,127}$/', $name) !== 1) {
                throw new \InvalidArgumentException("Invalid workflow step name {$name}.");
            }
            $this->activity = \Closure::fromCallable($activity);
            $this->compensation = $compensation === null
                ? null
                : \Closure::fromCallable($compensation);
        }
    }

    final readonly class Definition
    {
        /** @var list<Step> */
        public array $steps;

        /** @param list<Step> $steps */
        public function __construct(
            public string $name,
            public int $version,
            array $steps,
        ) {
            if (preg_match('/^[a-z][a-z0-9._-]{0,127}$/', $name) !== 1) {
                throw new \InvalidArgumentException("Invalid workflow name {$name}.");
            }
            if ($version < 1 || $steps === []) {
                throw new \InvalidArgumentException('Workflow version and steps must be positive.');
            }
            $names = array_map(static fn (Step $step): string => $step->name, $steps);
            if (count($names) !== count(array_unique($names))) {
                throw new \InvalidArgumentException('Workflow step names must be unique.');
            }
            $this->steps = $steps;
        }
    }

    final readonly class Instance
    {
        /**
         * @param array<string, mixed> $input
         * @param array<string, mixed>|null $result
         */
        public function __construct(
            public string $id,
            public string $definition,
            public int $version,
            public InstanceState $state,
            public array $input,
            public ?array $result,
            public ?string $error,
            public ?float $nextRunAt,
            public string $idempotencyKey,
        ) {
        }
    }

    final class Store
    {
        private readonly \PDO $database;

        public function __construct(string $path)
        {
            if ($path === '') {
                throw new \InvalidArgumentException('Workflow database path cannot be empty.');
            }
            $directory = dirname($path);
            if (!is_dir($directory) && !mkdir($directory, 0700, true) && !is_dir($directory)) {
                throw new \RuntimeException("Cannot create workflow directory {$directory}.");
            }
            $this->database = new \PDO("sqlite:{$path}", options: [
                \PDO::ATTR_ERRMODE => \PDO::ERRMODE_EXCEPTION,
                \PDO::ATTR_DEFAULT_FETCH_MODE => \PDO::FETCH_ASSOC,
                \PDO::ATTR_STRINGIFY_FETCHES => false,
            ]);
            $this->database->exec('PRAGMA journal_mode = WAL');
            $this->database->exec('PRAGMA synchronous = FULL');
            $this->database->exec('PRAGMA busy_timeout = 5000');
            $this->migrate();
        }

        /** @param array<string, mixed> $input */
        public function create(
            Definition $definition,
            array $input,
            string $idempotencyKey,
        ): Instance {
            if ($idempotencyKey === '' || strlen($idempotencyKey) > 255) {
                throw new \InvalidArgumentException('Workflow idempotency key must contain 1 to 255 bytes.');
            }
            $existing = $this->byIdempotency($definition->name, $idempotencyKey);
            if ($existing !== null) {
                if ($existing->version !== $definition->version) {
                    throw new \LogicException('Workflow idempotency key belongs to another definition version.');
                }
                return $existing;
            }

            $id = bin2hex(random_bytes(16));
            $now = microtime(true);
            $this->database->beginTransaction();
            try {
                $statement = $this->database->prepare(
                    'INSERT INTO pam_workflow_instances
                    (id, definition, version, state, input_json, result_json, error, next_run_at, idempotency_key, created_at, updated_at)
                    VALUES (:id, :definition, :version, :state, :input, NULL, NULL, NULL, :key, :now, :now)',
                );
                $statement->execute([
                    'id' => $id,
                    'definition' => $definition->name,
                    'version' => $definition->version,
                    'state' => InstanceState::Pending->value,
                    'input' => self::encode($input),
                    'key' => $idempotencyKey,
                    'now' => $now,
                ]);
                $stepStatement = $this->database->prepare(
                    'INSERT INTO pam_workflow_steps
                    (instance_id, position, name, state, attempts, result_json, error)
                    VALUES (:instance, :position, :name, :state, 0, NULL, NULL)',
                );
                foreach ($definition->steps as $position => $step) {
                    $stepStatement->execute([
                        'instance' => $id,
                        'position' => $position,
                        'name' => $step->name,
                        'state' => StepState::Pending->value,
                    ]);
                }
                $this->database->commit();
            } catch (\Throwable $error) {
                if ($this->database->inTransaction()) {
                    $this->database->rollBack();
                }
                $raced = $this->byIdempotency($definition->name, $idempotencyKey);
                if ($raced !== null) {
                    return $raced;
                }
                throw $error;
            }
            return $this->find($id);
        }

        public function find(string $id): Instance
        {
            $statement = $this->database->prepare(
                'SELECT * FROM pam_workflow_instances WHERE id = :id',
            );
            $statement->execute(['id' => $id]);
            $row = $statement->fetch();
            if ($row === false) {
                throw new \OutOfBoundsException("Workflow instance {$id} does not exist.");
            }
            return self::instance(self::instanceRow($row));
        }

        /** @return list<array{name: string, position: int, state: int, attempts: int, result_json: ?string, error: ?string}> */
        public function steps(string $instanceId): array
        {
            $statement = $this->database->prepare(
                'SELECT name, position, state, attempts, result_json, error
                FROM pam_workflow_steps WHERE instance_id = :id ORDER BY position',
            );
            $statement->execute(['id' => $instanceId]);
            $steps = [];
            foreach ($statement->fetchAll() as $row) {
                $steps[] = self::stepRow($row);
            }
            return $steps;
        }

        /** @param array<string, mixed>|null $result */
        public function transition(
            string $id,
            InstanceState $state,
            ?array $result = null,
            ?string $error = null,
            ?float $nextRunAt = null,
        ): void {
            $statement = $this->database->prepare(
                'UPDATE pam_workflow_instances
                SET state = :state, result_json = :result, error = :error,
                    next_run_at = :next, updated_at = :updated
                WHERE id = :id',
            );
            $statement->execute([
                'id' => $id,
                'state' => $state->value,
                'result' => $result === null ? null : self::encode($result),
                'error' => $error,
                'next' => $nextRunAt,
                'updated' => microtime(true),
            ]);
        }

        public function startStep(string $instanceId, string $name): int
        {
            $statement = $this->database->prepare(
                'UPDATE pam_workflow_steps
                SET state = :state, attempts = attempts + 1, error = NULL
                WHERE instance_id = :instance AND name = :name',
            );
            $statement->execute([
                'state' => StepState::Running->value,
                'instance' => $instanceId,
                'name' => $name,
            ]);
            $attempt = $this->database->prepare(
                'SELECT attempts FROM pam_workflow_steps
                WHERE instance_id = :instance AND name = :name',
            );
            $attempt->execute(['instance' => $instanceId, 'name' => $name]);
            $value = $attempt->fetchColumn();
            if (!is_int($value)) {
                throw new \RuntimeException('Workflow step attempt counter is invalid.');
            }
            return $value;
        }

        public function finishStep(
            string $instanceId,
            string $name,
            StepState $state,
            mixed $result = null,
            ?string $error = null,
        ): void {
            $statement = $this->database->prepare(
                'UPDATE pam_workflow_steps
                SET state = :state, result_json = :result, error = :error
                WHERE instance_id = :instance AND name = :name',
            );
            $statement->execute([
                'state' => $state->value,
                'result' => $result === null ? null : self::encode($result),
                'error' => $error,
                'instance' => $instanceId,
                'name' => $name,
            ]);
        }

        private function byIdempotency(string $definition, string $key): ?Instance
        {
            $statement = $this->database->prepare(
                'SELECT * FROM pam_workflow_instances
                WHERE definition = :definition AND idempotency_key = :key',
            );
            $statement->execute(['definition' => $definition, 'key' => $key]);
            $row = $statement->fetch();
            return $row === false ? null : self::instance(self::instanceRow($row));
        }

        /**
         * @param array{
         *   id: string,
         *   definition: string,
         *   version: int,
         *   state: int,
         *   input_json: string,
         *   result_json: ?string,
         *   error: ?string,
         *   next_run_at: int|float|null,
         *   idempotency_key: string
         * } $row
         */
        private static function instance(array $row): Instance
        {
            $input = self::decode($row['input_json']);
            $result = $row['result_json'] === null
                ? null
                : self::decode($row['result_json']);
            if (!is_array($input) || ($result !== null && !is_array($result))) {
                throw new \UnexpectedValueException('Workflow persistence contains invalid JSON.');
            }
            $input = self::stringMap($input, 'input');
            $result = $result === null ? null : self::stringMap($result, 'result');
            return new Instance(
                $row['id'],
                $row['definition'],
                $row['version'],
                InstanceState::from($row['state']),
                $input,
                $result,
                $row['error'],
                $row['next_run_at'] === null ? null : (float) $row['next_run_at'],
                $row['idempotency_key'],
            );
        }

        /**
         * @return array{
         *   id: string,
         *   definition: string,
         *   version: int,
         *   state: int,
         *   input_json: string,
         *   result_json: ?string,
         *   error: ?string,
         *   next_run_at: int|float|null,
         *   idempotency_key: string
         * }
         */
        private static function instanceRow(mixed $row): array
        {
            if (!is_array($row)) {
                throw new \UnexpectedValueException('Workflow instance row has an invalid shape.');
            }
            $resultJson = $row['result_json'] ?? null;
            $error = $row['error'] ?? null;
            $nextRunAt = $row['next_run_at'] ?? null;
            if (
                !is_string($row['id'] ?? null)
                || !is_string($row['definition'] ?? null)
                || !is_int($row['version'] ?? null)
                || !is_int($row['state'] ?? null)
                || !is_string($row['input_json'] ?? null)
                || !(is_string($resultJson) || $resultJson === null)
                || !(is_string($error) || $error === null)
                || !(is_int($nextRunAt) || is_float($nextRunAt) || $nextRunAt === null)
                || !is_string($row['idempotency_key'] ?? null)
            ) {
                throw new \UnexpectedValueException('Workflow instance row has an invalid shape.');
            }
            return [
                'id' => $row['id'],
                'definition' => $row['definition'],
                'version' => $row['version'],
                'state' => $row['state'],
                'input_json' => $row['input_json'],
                'result_json' => $resultJson,
                'error' => $error,
                'next_run_at' => $nextRunAt,
                'idempotency_key' => $row['idempotency_key'],
            ];
        }

        /** @return array{name: string, position: int, state: int, attempts: int, result_json: ?string, error: ?string} */
        private static function stepRow(mixed $row): array
        {
            if (!is_array($row)) {
                throw new \UnexpectedValueException('Workflow step row has an invalid shape.');
            }
            $resultJson = $row['result_json'] ?? null;
            $error = $row['error'] ?? null;
            if (
                !is_string($row['name'] ?? null)
                || !is_int($row['position'] ?? null)
                || !is_int($row['state'] ?? null)
                || !is_int($row['attempts'] ?? null)
                || !(is_string($resultJson) || $resultJson === null)
                || !(is_string($error) || $error === null)
            ) {
                throw new \UnexpectedValueException('Workflow step row has an invalid shape.');
            }
            return [
                'name' => $row['name'],
                'position' => $row['position'],
                'state' => $row['state'],
                'attempts' => $row['attempts'],
                'result_json' => $resultJson,
                'error' => $error,
            ];
        }

        /**
         * @param array<array-key, mixed> $value
         * @return array<string, mixed>
         */
        private static function stringMap(array $value, string $label): array
        {
            $normalized = [];
            foreach ($value as $key => $item) {
                if (!is_string($key)) {
                    throw new \UnexpectedValueException("Workflow {$label} must be an object.");
                }
                $normalized[$key] = $item;
            }
            return $normalized;
        }

        private function migrate(): void
        {
            $this->database->exec(
                'CREATE TABLE IF NOT EXISTS pam_workflow_instances (
                    id TEXT PRIMARY KEY,
                    definition TEXT NOT NULL,
                    version INTEGER NOT NULL,
                    state INTEGER NOT NULL,
                    input_json TEXT NOT NULL,
                    result_json TEXT,
                    error TEXT,
                    next_run_at REAL,
                    idempotency_key TEXT NOT NULL,
                    created_at REAL NOT NULL,
                    updated_at REAL NOT NULL,
                    UNIQUE (definition, idempotency_key)
                )',
            );
            $this->database->exec(
                'CREATE TABLE IF NOT EXISTS pam_workflow_steps (
                    instance_id TEXT NOT NULL,
                    position INTEGER NOT NULL,
                    name TEXT NOT NULL,
                    state INTEGER NOT NULL,
                    attempts INTEGER NOT NULL,
                    result_json TEXT,
                    error TEXT,
                    PRIMARY KEY (instance_id, name),
                    FOREIGN KEY (instance_id) REFERENCES pam_workflow_instances(id) ON DELETE CASCADE
                )',
            );
            $this->database->exec(
                'CREATE INDEX IF NOT EXISTS pam_workflow_due
                ON pam_workflow_instances (state, next_run_at)',
            );
        }

        private static function encode(mixed $value): string
        {
            return json_encode(
                $value,
                JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE,
            );
        }

        private static function decode(string $value): mixed
        {
            return json_decode($value, true, 64, JSON_THROW_ON_ERROR);
        }
    }

    final class Engine
    {
        /** @var array<string, Definition> */
        private array $definitions = [];

        public function __construct(private readonly Store $store)
        {
        }

        public function register(Definition $definition): self
        {
            $key = "{$definition->name}:{$definition->version}";
            if (isset($this->definitions[$key])) {
                throw new \LogicException("Workflow definition {$key} is already registered.");
            }
            $this->definitions[$key] = $definition;
            return $this;
        }

        /** @param array<string, mixed> $input */
        public function start(
            string $definition,
            array $input,
            string $idempotencyKey,
            ?int $version = null,
        ): Instance {
            $resolved = $this->definition($definition, $version);
            $instance = $this->store->create($resolved, $input, $idempotencyKey);
            return $this->run($instance->id);
        }

        public function run(string $instanceId): Instance
        {
            $instance = $this->store->find($instanceId);
            if (in_array($instance->state, [
                InstanceState::Completed,
                InstanceState::Failed,
                InstanceState::Compensated,
            ], true)) {
                return $instance;
            }
            if ($instance->nextRunAt !== null && $instance->nextRunAt > microtime(true)) {
                return $instance;
            }
            $definition = $this->definition($instance->definition, $instance->version);
            $this->transition($instance->id, InstanceState::Running);
            $persistedSteps = $this->store->steps($instance->id);
            $results = [];

            foreach ($persistedSteps as $position => $persisted) {
                $step = $definition->steps[$position] ?? null;
                if (!$step instanceof Step || $step->name !== $persisted['name']) {
                    throw new \LogicException('Workflow definition no longer matches persisted history.');
                }
                if ($persisted['state'] === StepState::Completed->value) {
                    $results[$step->name] = $persisted['result_json'] === null
                        ? null
                        : json_decode($persisted['result_json'], true, 64, JSON_THROW_ON_ERROR);
                    continue;
                }
                $attempt = $this->store->startStep($instance->id, $step->name);
                try {
                    $result = ($step->activity)(new Context($instance->id, $instance->input, $results));
                    $this->store->finishStep(
                        $instance->id,
                        $step->name,
                        StepState::Completed,
                        $result,
                    );
                    $results[$step->name] = $result;
                } catch (\Throwable $error) {
                    if ($attempt < $step->retry->maxAttempts) {
                        $next = microtime(true) + $step->retry->delayAfter($attempt);
                        $this->store->finishStep(
                            $instance->id,
                            $step->name,
                            StepState::Waiting,
                            error: self::error($error),
                        );
                        $this->transition(
                            $instance->id,
                            InstanceState::Waiting,
                            error: self::error($error),
                            nextRunAt: $next,
                        );
                        return $this->store->find($instance->id);
                    }
                    $this->store->finishStep(
                        $instance->id,
                        $step->name,
                        StepState::Failed,
                        error: self::error($error),
                    );
                    return $this->compensate($definition, $instance, $results, $error);
                }
            }

            $this->transition($instance->id, InstanceState::Completed, $results);
            return $this->store->find($instance->id);
        }

        /** @param array<string, mixed> $results */
        private function compensate(
            Definition $definition,
            Instance $instance,
            array $results,
            \Throwable $cause,
        ): Instance {
            $this->transition(
                $instance->id,
                InstanceState::Compensating,
                error: self::error($cause),
            );
            try {
                foreach (array_reverse($definition->steps) as $step) {
                    if (!array_key_exists($step->name, $results) || $step->compensation === null) {
                        continue;
                    }
                    ($step->compensation)(
                        new Context($instance->id, $instance->input, $results),
                        $results[$step->name],
                    );
                    $this->store->finishStep(
                        $instance->id,
                        $step->name,
                        StepState::Compensated,
                        $results[$step->name],
                    );
                }
                $this->transition(
                    $instance->id,
                    InstanceState::Compensated,
                    error: self::error($cause),
                );
            } catch (\Throwable $compensationError) {
                $this->transition(
                    $instance->id,
                    InstanceState::Failed,
                    error: self::error($cause) . '; compensation: ' . self::error($compensationError),
                );
            }
            return $this->store->find($instance->id);
        }

        private function definition(string $name, ?int $version): Definition
        {
            if ($version !== null) {
                return $this->definitions["{$name}:{$version}"]
                    ?? throw new \OutOfBoundsException("Workflow {$name}:{$version} is not registered.");
            }
            $matches = array_values(array_filter(
                $this->definitions,
                static fn (Definition $definition): bool => $definition->name === $name,
            ));
            if ($matches === []) {
                throw new \OutOfBoundsException("Workflow {$name} is not registered.");
            }
            usort(
                $matches,
                static fn (Definition $left, Definition $right): int => $right->version <=> $left->version,
            );
            return $matches[0];
        }

        /** @param array<string, mixed>|null $result */
        private function transition(
            string $instanceId,
            InstanceState $state,
            ?array $result = null,
            ?string $error = null,
            ?float $nextRunAt = null,
        ): void {
            $this->store->transition($instanceId, $state, $result, $error, $nextRunAt);
            \Pam\Diagnostics\Channel::publish(
                \Pam\Diagnostics\EventKind::WorkflowTransition,
                ['instanceId' => $instanceId, 'state' => $state->value],
            );
        }

        private static function error(\Throwable $error): string
        {
            return $error::class . ': ' . $error->getMessage();
        }
    }
}
