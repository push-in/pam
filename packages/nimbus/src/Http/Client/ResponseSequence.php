<?php

declare(strict_types=1);

namespace Nimbus\Http\Client;

/** Responses returned in order by a faked URL, such as a failure followed by a success. */
final class ResponseSequence
{
    /** @var list<Response|ConnectionException> */
    private array $responses = [];

    private Response|ConnectionException|null $whenEmpty = null;

    /**
     * @param array<array-key, mixed>|string|null $body
     * @param array<string, string|list<string>> $headers
     */
    public function push(array|string|null $body = null, int $status = 200, array $headers = []): self
    {
        return $this->pushResponse(Http::response($body, $status, $headers));
    }

    public function pushStatus(int $status): self
    {
        return $this->push(null, $status);
    }

    public function pushResponse(Response|ConnectionException $response): self
    {
        $this->responses[] = $response;
        return $this;
    }

    public function pushFailedConnection(string $message = 'Connection refused.'): self
    {
        return $this->pushResponse(new ConnectionException($message));
    }

    /** Returned once the pushed responses run out, instead of failing the test. */
    public function whenEmpty(Response|ConnectionException $response): self
    {
        $this->whenEmpty = $response;
        return $this;
    }

    public function isEmpty(): bool
    {
        return $this->responses === [];
    }

    /** @throws ConnectionException When the next entry is a failed connection. */
    public function next(): Response
    {
        $next = array_shift($this->responses)
            ?? $this->whenEmpty
            ?? throw new \OutOfBoundsException('The HTTP response sequence is empty.');
        if ($next instanceof ConnectionException) {
            throw $next;
        }
        return $next;
    }
}
