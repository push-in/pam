<?php

declare(strict_types=1);

namespace Nimbus\Http\Client;

/**
 * Creates request builders over one transport. Bind it in the container to inject it, or
 * use the `Http` facade, which keeps one factory per worker.
 */
final class Factory
{
    private ?Transport $transport;

    public function __construct(?Transport $transport = null)
    {
        $this->transport = $transport;
    }

    /**
     * The PAM runtime client inside PAM (non-blocking in request Fibers), else
     * symfony/http-client (blocking).
     */
    public static function detectTransport(): Transport
    {
        if (PamTransport::available()) {
            return new PamTransport();
        }
        if (class_exists(\Symfony\Component\HttpClient\HttpClient::class)) {
            return new SymfonyTransport();
        }
        throw new \LogicException('No HTTP transport: run inside PAM or composer require symfony/http-client.');
    }

    public function transport(): Transport
    {
        return $this->transport ??= self::detectTransport();
    }

    public function request(): PendingRequest
    {
        return new PendingRequest($this->transport());
    }

    /**
     * Replaces the transport with a recording fake; see HttpFake for the stub forms.
     *
     * @param array<string, Response|ResponseSequence|ConnectionException|\Closure|array<array-key, mixed>|string>|\Closure $stubs
     */
    public function fake(array|\Closure $stubs = []): HttpFake
    {
        $fake = $this->transport instanceof HttpFake ? $this->transport : new HttpFake();
        $fake->stub($stubs);
        $this->transport = $fake;
        return $fake;
    }
}
