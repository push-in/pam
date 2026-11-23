<?php

declare(strict_types=1);

namespace Nimbus\Tests\Events\Fixtures;


final class Unqueueable
{
    public function __construct(public \stdClass $object)
    {
    }
}
