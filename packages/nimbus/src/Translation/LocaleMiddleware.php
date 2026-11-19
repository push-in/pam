<?php

declare(strict_types=1);

namespace Nimbus\Translation;

use Nimbus\Container\Container;
use Nimbus\Session\Session;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Picks the request's locale and registers it in the request scope: the `$queryParameter`
 * (`?lang=pt_BR`), then the session's `locale`, then `Accept-Language`, then the default.
 * Only supported locales are accepted. Sends `Content-Language` and `Vary: Accept-Language`.
 */
final readonly class LocaleMiddleware implements MiddlewareInterface
{
    public function __construct(
        private Translator $translator,
        private Container $container,
        private ?string $queryParameter = 'lang',
        private string $sessionKey = 'locale',
    ) {
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $locale = $this->fromQuery($request) ?? $this->fromSession() ?? $this->translator->negotiate($request->getHeader('accept-language'));
        $locale ??= $this->translator->locale();
        $this->container->scopedInstance(Locale::class, new Locale($locale));
        $response = $next->handle($request, $response);
        return $response
            ->header('content-language', str_replace('_', '-', $locale))
            ->addHeader('vary', 'accept-language');
    }

    private function fromQuery(Request $request): ?string
    {
        if ($this->queryParameter === null) {
            return null;
        }
        $value = $request->query()[$this->queryParameter] ?? null;
        return is_string($value) && $this->translator->supports($value) ? $this->supported($value) : null;
    }

    private function fromSession(): ?string
    {
        $session = $this->container->scopedValue(Session::class);
        $value = $session instanceof Session ? $session->get($this->sessionKey) : null;
        return is_string($value) && $this->translator->supports($value) ? $this->supported($value) : null;
    }

    private function supported(string $locale): string
    {
        foreach ($this->translator->supportedLocales() as $supported) {
            if (strtolower($supported) === strtolower(Locale::normalize($locale))) {
                return $supported;
            }
        }
        return Locale::normalize($locale);
    }
}
