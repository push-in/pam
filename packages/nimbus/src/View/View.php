<?php

declare(strict_types=1);

namespace Nimbus\View;

use Nimbus\Http\Responsable;
use Pam\Http\Request;
use Pam\Http\Response;

/** A template and its data; rendered when returned from a route or printed. */
final readonly class View implements Responsable, Htmlable, \Stringable
{
    /** @param array<string, mixed> $data */
    public function __construct(
        private ViewFactory $factory,
        public string $name,
        public array $data = [],
        public int $status = 200,
    ) {
    }

    /** @param string|array<string, mixed> $key */
    public function with(string|array $key, mixed $value = null): self
    {
        $data = is_array($key) ? $key + $this->data : [$key => $value] + $this->data;
        return new self($this->factory, $this->name, $data, $this->status);
    }

    public function withStatus(int $status): self
    {
        return new self($this->factory, $this->name, $this->data, $status);
    }

    public function render(): string
    {
        return $this->factory->render($this->name, $this->data);
    }

    public function toHtml(): string
    {
        return $this->render();
    }

    public function __toString(): string
    {
        return $this->render();
    }

    public function toResponse(Request $request, Response $response): Response
    {
        return $response
            ->status($this->status)
            ->header('content-type', 'text/html; charset=utf-8')
            ->send($this->render())
            ->header('content-type', 'text/html; charset=utf-8');
    }
}
