<?php

declare(strict_types=1);

namespace Nimbus\View;

/** Trusted HTML that `{{ }}` does not escape. */
readonly class HtmlString implements Htmlable, \Stringable
{
    public function __construct(private string $html = '')
    {
    }

    public function toHtml(): string
    {
        return $this->html;
    }

    public function isEmpty(): bool
    {
        return trim($this->html) === '';
    }

    public function isNotEmpty(): bool
    {
        return !$this->isEmpty();
    }

    public function __toString(): string
    {
        return $this->html;
    }
}
