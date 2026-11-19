<?php

declare(strict_types=1);

namespace Nimbus\Session;

/** One JSON file per session in a directory; files are replaced atomically. */
final readonly class FileSessionStore implements SessionStore
{
    /** @var \Closure(): int */
    private \Closure $clock;

    /** @param (\Closure(): int)|null $clock */
    public function __construct(
        private string $directory,
        ?\Closure $clock = null,
    ) {
        if ($directory === '') {
            throw new \InvalidArgumentException('The session directory is required.');
        }
        $this->clock = $clock ?? time(...);
    }

    public function read(string $id): ?array
    {
        $path = $this->path($id);
        if ($path === null || !is_file($path)) {
            return null;
        }
        $contents = @file_get_contents($path);
        $record = is_string($contents) ? SessionPayload::decode($contents) : null;
        if ($record === null || !is_int($record['expires_at'] ?? null) || $record['expires_at'] <= ($this->clock)()) {
            return null;
        }
        $data = $record['data'] ?? null;
        return is_array($data) ? SessionPayload::normalize($data) : null;
    }

    public function write(string $id, array $data, int $lifetimeSeconds): void
    {
        $path = $this->path($id) ?? throw new \InvalidArgumentException('Invalid session id.');
        if (!is_dir($this->directory) && !@mkdir($this->directory, 0o700, true) && !is_dir($this->directory)) {
            throw new \RuntimeException("Could not create the session directory {$this->directory}.");
        }
        $temporary = $path . '.' . bin2hex(random_bytes(4)) . '.tmp';
        $encoded = SessionPayload::encode(['expires_at' => ($this->clock)() + $lifetimeSeconds, 'data' => $data]);
        if (file_put_contents($temporary, $encoded, LOCK_EX) !== strlen($encoded) || !rename($temporary, $path)) {
            @unlink($temporary);
            throw new \RuntimeException('Could not write the session file.');
        }
        @chmod($path, 0o600);
    }

    public function destroy(string $id): void
    {
        $path = $this->path($id);
        if ($path !== null) {
            @unlink($path);
        }
    }

    public function gc(): int
    {
        $removed = 0;
        foreach (glob($this->directory . '/*') ?: [] as $file) {
            $id = basename($file);
            if (SessionPayload::validId($id) && $this->read($id) === null) {
                @unlink($file);
                ++$removed;
            }
        }
        return $removed;
    }

    private function path(string $id): ?string
    {
        return SessionPayload::validId($id) ? $this->directory . '/' . $id : null;
    }
}
