<?php

declare(strict_types=1);

namespace Nimbus\View;

/** Output that is already HTML: `{{ }}` prints it without escaping. */
interface Htmlable
{
    public function toHtml(): string;
}
