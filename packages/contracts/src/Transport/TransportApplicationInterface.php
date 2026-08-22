<?php

declare(strict_types=1);

namespace Pam\Contracts\Transport;

use Pam\Contracts\Http\ApplicationInterface;

interface TransportApplicationInterface extends ApplicationInterface
{
    public function transport(TransportProviderInterface $provider): self;

    /** @return array<string, TransportProviderInterface> */
    public function transports(): array;
}
