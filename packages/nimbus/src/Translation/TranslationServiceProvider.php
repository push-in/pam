<?php

declare(strict_types=1);

namespace Nimbus\Translation;

use Nimbus\App;
use Pam\Contracts\Http\ApplicationInterface;
use Pam\Contracts\Package\ServiceProviderInterface;

/** Registers the Translator over a `lang/` directory and, by default, LocaleMiddleware. */
final readonly class TranslationServiceProvider implements ServiceProviderInterface
{
    /**
     * @param list<string> $fallbackLocales
     * @param list<string>|null $supportedLocales
     */
    public function __construct(
        private string $directory,
        private string $defaultLocale = 'en',
        private array $fallbackLocales = [],
        private ?array $supportedLocales = null,
        private bool $middleware = true,
    ) {
    }

    public function register(ApplicationInterface $application): void
    {
        if (!$application instanceof App) {
            throw new \InvalidArgumentException('TranslationServiceProvider requires Nimbus\\App.');
        }
        $container = $application->container();
        $translator = (new Translator($this->directory, $this->defaultLocale, $this->fallbackLocales, $container, $this->supportedLocales))->useAsDefault();
        $container->instance(Translator::class, $translator);
        $middleware = new LocaleMiddleware($translator, $container);
        $container->instance(LocaleMiddleware::class, $middleware);
        if ($this->middleware) {
            $application->middleware($middleware);
        }
    }

    public function boot(ApplicationInterface $application): void
    {
    }
}
