<?php

declare(strict_types=1);

namespace Nimbus\View;

/**
 * The attributes of a component tag that are not props (`$attributes`). Printed with
 * `{{ $attributes }}`, values escaped; `true` renders a bare attribute, `false`/`null` none.
 *
 * @implements \IteratorAggregate<string, mixed>
 */
final readonly class AttributeBag implements Htmlable, \Stringable, \IteratorAggregate, \Countable
{
    /** @param array<string, mixed> $attributes */
    public function __construct(private array $attributes = [])
    {
    }

    /**
     * Defaults under the given attributes; `class` (and `style`) values are combined
     * (defaults first) instead of replaced.
     *
     * @param array<string, mixed> $defaults
     */
    public function merge(array $defaults): self
    {
        $merged = $defaults;
        foreach ($this->attributes as $name => $value) {
            if (($name === 'class' || $name === 'style') && isset($defaults[$name]) && is_scalar($value) && is_scalar($defaults[$name])) {
                $separator = $name === 'class' ? ' ' : '; ';
                $merged[$name] = trim(rtrim((string) $defaults[$name], '; ') . $separator . (string) $value, '; ');
                continue;
            }
            $merged[$name] = $value;
        }
        return new self($merged);
    }

    /**
     * Adds conditional classes: `['p-4', 'font-bold' => $active]`.
     *
     * @param array<array-key, mixed> $classes
     */
    public function class(array $classes): self
    {
        return $this->merge(['class' => self::classList($classes)]);
    }

    /** @param array<array-key, mixed> $classes */
    public static function classList(array $classes): string
    {
        $list = [];
        foreach ($classes as $key => $value) {
            if (is_int($key)) {
                if (is_string($value) && $value !== '') {
                    $list[] = $value;
                }
            } elseif ($value) {
                $list[] = $key;
            }
        }
        return implode(' ', $list);
    }

    public function get(string $name, mixed $default = null): mixed
    {
        return array_key_exists($name, $this->attributes) ? $this->attributes[$name] : $default;
    }

    public function has(string $name): bool
    {
        return array_key_exists($name, $this->attributes);
    }

    /** @param string|list<string> $names */
    public function only(string|array $names): self
    {
        return new self(array_intersect_key($this->attributes, array_flip((array) $names)));
    }

    /** @param string|list<string> $names */
    public function except(string|array $names): self
    {
        return new self(array_diff_key($this->attributes, array_flip((array) $names)));
    }

    /** Attributes whose name starts with the prefix (`wire:`, `data-`). */
    public function whereStartsWith(string $prefix): self
    {
        return new self(array_filter($this->attributes, static fn (string $name): bool => str_starts_with($name, $prefix), ARRAY_FILTER_USE_KEY));
    }

    /** @return array<string, mixed> */
    public function getAttributes(): array
    {
        return $this->attributes;
    }

    public function isEmpty(): bool
    {
        return $this->attributes === [];
    }

    public function count(): int
    {
        return count($this->attributes);
    }

    /** @return \ArrayIterator<string, mixed> */
    public function getIterator(): \ArrayIterator
    {
        return new \ArrayIterator($this->attributes);
    }

    public function toHtml(): string
    {
        $html = [];
        foreach ($this->attributes as $name => $value) {
            if ($value === false || $value === null) {
                continue;
            }
            if ($value === true) {
                $html[] = Renderer::escape($name);
                continue;
            }
            $html[] = Renderer::escape($name) . '="' . Renderer::escape($value) . '"';
        }
        return implode(' ', $html);
    }

    public function __toString(): string
    {
        return $this->toHtml();
    }
}
