<?php

declare(strict_types=1);

namespace Nimbus\Tests\Redis;

use Nimbus\Redis\Connection;

/** A minimal RESP client for the opt-in Redis tests. */
final class SocketConnection implements Connection
{
    /** @var resource */
    private $stream;

    public function __construct(string $address)
    {
        $stream = stream_socket_client('tcp://' . $address, $code, $message, 2.0);
        if ($stream === false) {
            throw new \RuntimeException("Cannot reach Redis at {$address}: {$message}");
        }
        $this->stream = $stream;
    }

    public function command(array $arguments): mixed
    {
        $payload = '*' . count($arguments) . "\r\n";
        foreach ($arguments as $argument) {
            $argument = (string) $argument;
            $payload .= '$' . strlen($argument) . "\r\n" . $argument . "\r\n";
        }
        fwrite($this->stream, $payload);
        return $this->read();
    }

    private function read(): mixed
    {
        $line = fgets($this->stream);
        if ($line === false) {
            throw new \RuntimeException('Redis closed the connection.');
        }
        $line = substr($line, 0, -2);
        $rest = substr($line, 1);
        return match ($line[0]) {
            '+' => $rest,
            '-' => throw new \RuntimeException($rest),
            ':' => (int) $rest,
            '$' => $rest === '-1' ? null : substr((string) fread($this->stream, max(1, (int) $rest + 2)), 0, -2),
            '*' => (int) $rest < 1 ? [] : array_map(fn (): mixed => $this->read(), range(1, (int) $rest)),
            default => throw new \RuntimeException('Unexpected Redis reply.'),
        };
    }
}
