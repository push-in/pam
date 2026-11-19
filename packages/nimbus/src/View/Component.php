<?php

declare(strict_types=1);

namespace Nimbus\View;

/**
 * A component with a class (`App\Views\Alert` for `<x-alert>`). Constructor parameters are
 * filled from the tag attributes (kebab-case attributes match camelCase parameters; class
 * typed parameters come from the container); the other attributes stay in `$attributes`.
 * Public properties and methods are variables of the template returned by render().
 */
abstract class Component
{
    public AttributeBag $attributes;

    /** The template name (`components.alert`), a View or HTML. */
    abstract public function render(): string|View|Htmlable;

    public function shouldRender(): bool
    {
        return true;
    }

    /** @internal */
    final public function withAttributes(AttributeBag $attributes): static
    {
        $this->attributes = $attributes;
        return $this;
    }
}
