<?php

declare(strict_types=1);

namespace Pam\Contracts\Transport;

interface TransportProviderInterface
{
    public function descriptor(): TransportDescriptor;

    public function start(TransportContext $context): void;

    /** @return iterable<TransportMessage> */
    public function receive(int $maximum, int $waitMilliseconds): iterable;

    /** @param array<string, string> $headers */
    public function publish(string $topic, string $payload, array $headers = []): void;

    public function acknowledge(
        TransportMessage $message,
        MessageDisposition $disposition,
        ?int $retryAfterMilliseconds = null,
    ): void;

    public function stop(): void;
}
