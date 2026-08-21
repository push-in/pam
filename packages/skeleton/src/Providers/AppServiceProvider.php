<?php

declare(strict_types=1);

namespace App\Providers;

use App\Repositories\EloquentProductRepository;
use App\Repositories\ProductRepository;
use Pam\App;
use Pam\Contracts\Http\ApplicationInterface;
use Pam\Contracts\Package\ServiceProviderInterface;

final readonly class AppServiceProvider implements ServiceProviderInterface
{
    public function register(ApplicationInterface $application): void
    {
        if (!$application instanceof App) {
            throw new \InvalidArgumentException('AppServiceProvider requires Pam\\App.');
        }

        $application->container()->bind(ProductRepository::class, EloquentProductRepository::class);
    }

    public function boot(ApplicationInterface $application): void
    {
        if (!$application instanceof App) {
            throw new \InvalidArgumentException('AppServiceProvider requires Pam\\App.');
        }
    }
}
