<?php

declare(strict_types=1);

namespace Pam\Api\Jobs;

final readonly class JobCodec
{
    private const int SCHEMA_VERSION = 1;
    private const int MAXIMUM_BYTES = 65_536;
    private const int MAXIMUM_DEPTH = 16;

    /** @var array<non-empty-string, class-string<SerializableJob>> */
    private array $jobs;

    /** @param list<class-string<SerializableJob>> $jobs */
    public function __construct(array $jobs)
    {
        if (count($jobs) > 1_000) {
            throw new \InvalidArgumentException('A job codec may register at most 1,000 job types.');
        }
        $registered = [];
        foreach ($jobs as $job) {
            $name = $job::jobName();
            if (strlen($name) > 128 || preg_match('/^[a-z][a-z0-9]*(?:[._-][a-z0-9]+)*$/D', $name) !== 1) {
                throw new \InvalidArgumentException('Job names must be bounded lowercase identifiers.');
            }
            if (isset($registered[$name])) {
                throw new \InvalidArgumentException("Duplicate job name: {$name}.");
            }
            $registered[$name] = $job;
        }
        $this->jobs = $registered;
    }

    public function encode(SerializableJob $job): string
    {
        $name = $job::jobName();
        if (($this->jobs[$name] ?? null) !== $job::class) {
            throw new \InvalidArgumentException('The job type is not registered in this codec.');
        }
        $json = json_encode([
            'schema' => self::SCHEMA_VERSION,
            'name' => $name,
            'payload' => $job->toJobPayload(),
        ], JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES, self::MAXIMUM_DEPTH);
        if (strlen($json) > self::MAXIMUM_BYTES) {
            throw new \LengthException('Encoded job exceeds the 64 KiB limit.');
        }
        return $json;
    }

    public function decode(string $json): SerializableJob
    {
        if ($json === '' || strlen($json) > self::MAXIMUM_BYTES) {
            throw new \LengthException('Encoded job must contain at most 64 KiB.');
        }
        $decoded = json_decode($json, true, self::MAXIMUM_DEPTH, JSON_THROW_ON_ERROR);
        if (!is_array($decoded)
            || array_keys($decoded) !== ['schema', 'name', 'payload']
            || ($decoded['schema'] ?? null) !== self::SCHEMA_VERSION
            || !is_string($decoded['name'] ?? null)
            || !is_array($decoded['payload'] ?? null)
        ) {
            throw new \UnexpectedValueException('Encoded job envelope is invalid.');
        }
        $class = $this->jobs[$decoded['name']] ?? null;
        if ($class === null) {
            throw new \UnexpectedValueException('Encoded job type is not registered.');
        }
        /** @var array<string, mixed> $payload */
        $payload = $decoded['payload'];
        return $class::fromJobPayload($payload);
    }
}
