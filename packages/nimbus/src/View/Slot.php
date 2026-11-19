<?php

declare(strict_types=1);

namespace Nimbus\View;

/** The content passed between component tags (`$slot`) or in `<x-slot:name>` (`$name`). */
final readonly class Slot extends HtmlString
{
    public AttributeBag $attributes;

    public function __construct(string $html = '', ?AttributeBag $attributes = null)
    {
        parent::__construct($html);
        $this->attributes = $attributes ?? new AttributeBag();
    }
}
