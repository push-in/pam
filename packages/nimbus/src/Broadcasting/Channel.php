<?php

declare(strict_types=1);

namespace Nimbus\Broadcasting;

/** A public channel: anyone may subscribe. */
class Channel implements \Stringable
{
    public readonly string $name;

    public function __construct(string $name)
    {
        if ($name === '' || strlen($name) > 200 || preg_match('/^[A-Za-z0-9_\-=@,.;]+$/D', $name) !== 1) {
            throw new \InvalidArgumentException("Invalid broadcast channel name: {$name}.");
        }
        $this->name = $name;
    }

    public function __toString(): string
    {
        return $this->name;
    }
}
