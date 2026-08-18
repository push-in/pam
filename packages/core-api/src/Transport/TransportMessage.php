<?php

declare(strict_types=1);

namespace Pam\Contracts\Transport;

final readonly class TransportMessage
{
    /** @var array<string, string> */
    public array $headers;

    /** @param array<array-key, mixed> $headers */
    public function __construct(
        public string $id,
        public string $topic,
        public string $payload,
        array $headers = [],
        public int $attempt = 1,
        public ?int $receivedAtUnixMs = null,
    ) {
        if ($id === '' || strlen($id) > 256 || str_contains($id, "\0")) {
            throw new \InvalidArgumentException('Transport message ID is invalid.');
        }
        if ($topic === '' || strlen($topic) > 256 || preg_match('/^[A-Za-z0-9_.:\/-]+$/D', $topic) !== 1) {
            throw new \InvalidArgumentException('Transport message topic is invalid.');
        }
        if ($attempt < 1 || $attempt > 1_000) {
            throw new \InvalidArgumentException('Transport delivery attempt is outside the supported range.');
        }
        if (count($headers) > 64) {
            throw new \InvalidArgumentException('Transport message cannot contain more than 64 headers.');
        }
        foreach ($headers as $name => $value) {
            if (!is_string($name) || !is_string($value)
                || preg_match('/^[a-z0-9][a-z0-9_-]{0,63}$/D', $name) !== 1
                || strlen($value) > 8_192
                || str_contains($value, "\0")
            ) {
                throw new \InvalidArgumentException('Transport message contains an invalid header.');
            }
        }
        /** @var array<string, string> $headers */
        $this->headers = $headers;
    }
}
