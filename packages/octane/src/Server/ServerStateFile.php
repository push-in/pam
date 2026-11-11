<?php

declare(strict_types=1);

namespace Pam\Octane\Server;

use RuntimeException;

/**
 * The state file of a `php artisan octane:start --server=pam` server: the
 * PID of the `pam start` supervisor and the options it was started with.
 */
class ServerStateFile
{
    public function __construct(protected string $path)
    {
    }

    /** @return array{masterProcessId: ?int, state: array<string, mixed>} */
    public function read(): array
    {
        $contents = is_readable($this->path) ? file_get_contents($this->path) : false;
        $state = is_string($contents) ? json_decode($contents, true) : null;
        $state = is_array($state) ? $state : [];
        $pid = $state['masterProcessId'] ?? null;
        $values = $state['state'] ?? [];

        $normalized = [];
        foreach (is_array($values) ? $values : [] as $key => $value) {
            $normalized[(string) $key] = $value;
        }

        return [
            'masterProcessId' => is_int($pid) && $pid > 0 ? $pid : null,
            'state' => $normalized,
        ];
    }

    public function writeProcessId(int $masterProcessId): void
    {
        $this->write(array_merge($this->read(), ['masterProcessId' => $masterProcessId]));
    }

    /** @param array<string, mixed> $newState */
    public function writeState(array $newState): void
    {
        $this->write(array_merge($this->read(), ['state' => $newState]));
    }

    public function delete(): bool
    {
        return is_writable($this->path) && unlink($this->path);
    }

    public function path(): string
    {
        return $this->path;
    }

    /** @param array<string, mixed> $contents */
    private function write(array $contents): void
    {
        if (!is_writable($this->path) && !is_writable(dirname($this->path))) {
            throw new RuntimeException('Unable to write to process ID file.');
        }

        file_put_contents($this->path, json_encode($contents, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR));
    }
}
