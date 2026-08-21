<?php

declare(strict_types=1);

namespace Pam\Api\Http;

use Pam\Http\Response;

final readonly class ResponseSnapshot
{
    /** @param array<string, list<string>> $headers */
    public function __construct(
        public int $status,
        public array $headers,
        public string $body,
    ) {
    }

    public static function capture(Response $response): self
    {
        $export = $response->export();
        return new self($export['status'], $export['headers'], $export['body']);
    }

    public function restore(Response $response): Response
    {
        $response->status($this->status);
        foreach ($this->headers as $name => $values) {
            foreach ($values as $index => $value) {
                $index === 0 ? $response->header($name, $value) : $response->addHeader($name, $value);
            }
        }
        return $response->send($this->body);
    }
}

