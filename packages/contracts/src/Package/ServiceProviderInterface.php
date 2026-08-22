<?php

declare(strict_types=1);

namespace Pam\Contracts\Package;

use Pam\Contracts\Http\ApplicationInterface;

interface ServiceProviderInterface
{
    public function register(ApplicationInterface $application): void;

    public function boot(ApplicationInterface $application): void;
}
