<?php

declare(strict_types=1);

namespace Nimbus\View;

use Nimbus\App;
use Nimbus\Translation\Translator;
use Pam\Contracts\Http\ApplicationInterface;
use Pam\Contracts\Package\ServiceProviderInterface;

/**
 * Registers a ViewFactory over `resources/views` (or the given paths) as the container's
 * factory and the one used by `view()`.
 */
final readonly class ViewServiceProvider implements ServiceProviderInterface
{
    /**
     * @param string|list<string> $paths
     * @param bool $reload Recompiles a template when its file changes (development only).
     */
    public function __construct(
        private string|array $paths,
        private string $componentNamespace = 'App\\Views',
        private bool $reload = false,
    ) {
    }

    public function register(ApplicationInterface $application): void
    {
        if (!$application instanceof App) {
            throw new \InvalidArgumentException('ViewServiceProvider requires Nimbus\\App.');
        }
        $container = $application->container();
        $factory = new ViewFactory($this->paths, $container, $this->componentNamespace, $this->reload);
        $container->instance(ViewFactory::class, $factory->useAsDefault());
    }

    public function boot(ApplicationInterface $application): void
    {
        $container = $application instanceof App ? $application->container() : null;
        if ($container !== null && $container->has(Translator::class)) {
            $factory = $container->get(ViewFactory::class);
            $translator = $container->get(Translator::class);
            if ($factory instanceof ViewFactory && $translator instanceof Translator) {
                $factory->setTranslator($translator);
            }
        }
    }
}
