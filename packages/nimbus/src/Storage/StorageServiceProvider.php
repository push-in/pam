<?php

declare(strict_types=1);

namespace Nimbus\Storage;

use Nimbus\App;
use Nimbus\Security\UrlSigner;
use Pam\Contracts\Http\ApplicationInterface;
use Pam\Contracts\Package\ServiceProviderInterface;

final readonly class StorageServiceProvider implements ServiceProviderInterface
{
    /**
     * @param ?string $routes Registers the signed upload and download routes under this path,
     *        giving local disks temporary URLs (signed with APP_KEY unless `$signer` is given).
     */
    public function __construct(
        private StorageConfig $config,
        private ?string $routes = null,
        private ?UrlSigner $signer = null,
    ) {
    }

    public function register(ApplicationInterface $application): void
    {
        if (!$application instanceof App) {
            throw new \InvalidArgumentException('StorageServiceProvider requires Nimbus\\App.');
        }
        $container = $application->container();
        $signer = $this->signer;
        if ($signer !== null) {
            $container->instance(UrlSigner::class, $signer);
        } elseif ($this->routes !== null) {
            $resolved = $container->get(UrlSigner::class);
            $signer = $resolved instanceof UrlSigner ? $resolved : UrlSigner::fromEnvironment();
        }
        $storage = new StorageManager($this->config, $signer, $this->routes);
        $container
            ->instance(StorageConfig::class, $this->config)
            ->instance(StorageManager::class, $storage)
            ->singleton(Disk::class, static fn (): Disk => $storage->disk());
        if ($this->routes !== null) {
            (new StorageRoutes($storage))->register($application, $this->routes);
        }
    }

    public function boot(ApplicationInterface $application): void
    {
    }
}
