<?php

declare(strict_types=1);

namespace Nimbus\Session;

use Nimbus\App;
use Nimbus\Http\CookieCodec;
use Nimbus\Middleware\EncryptCookiesMiddleware;
use Nimbus\Middleware\MethodOverrideMiddleware;
use Nimbus\Middleware\StartSessionMiddleware;
use Nimbus\Middleware\VerifyCsrfTokenMiddleware;
use Pam\Contracts\Http\ApplicationInterface;
use Pam\Contracts\Package\ServiceProviderInterface;

/**
 * Web state for an application: binds the store, then (with `$middleware`) adds the global
 * middleware in order: method override, encrypted cookies (with a codec), session, CSRF.
 */
final readonly class SessionServiceProvider implements ServiceProviderInterface
{
    /** @param list<string> $csrfExcept Paths without CSRF checks (`/webhooks/*` matches by prefix). */
    public function __construct(
        private SessionStore $store,
        private SessionConfig $config = new SessionConfig(),
        private ?CookieCodec $cookies = null,
        private bool $middleware = true,
        private array $csrfExcept = [],
    ) {
    }

    public function register(ApplicationInterface $application): void
    {
        if (!$application instanceof App) {
            throw new \InvalidArgumentException('SessionServiceProvider requires Nimbus\\App.');
        }
        $container = $application->container();
        $container
            ->instance(SessionStore::class, $this->store)
            ->instance(SessionConfig::class, $this->config);
        $startSession = new StartSessionMiddleware($container, $this->store, $this->config);
        $container->instance(StartSessionMiddleware::class, $startSession);
        $csrf = new VerifyCsrfTokenMiddleware($container, $this->csrfExcept, secureCookie: $this->config->secure);
        $container->instance(VerifyCsrfTokenMiddleware::class, $csrf);
        if ($this->cookies !== null) {
            $container->instance(CookieCodec::class, $this->cookies);
            $container->instance(EncryptCookiesMiddleware::class, new EncryptCookiesMiddleware(
                $container,
                $this->cookies,
                [$this->config->cookie, 'XSRF-TOKEN'],
            ));
        }
        if (!$this->middleware) {
            return;
        }
        $application->middleware(new MethodOverrideMiddleware());
        if ($this->cookies !== null) {
            $application->middleware(EncryptCookiesMiddleware::class);
        }
        $application->middleware(StartSessionMiddleware::class)->middleware(VerifyCsrfTokenMiddleware::class);
    }

    public function boot(ApplicationInterface $application): void
    {
    }
}
