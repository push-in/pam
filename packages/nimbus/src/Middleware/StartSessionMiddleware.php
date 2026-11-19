<?php

declare(strict_types=1);

namespace Nimbus\Middleware;

use Nimbus\Container\Container;
use Nimbus\Http\Cookie;
use Nimbus\Http\FormData;
use Nimbus\Http\Redirect;
use Nimbus\Http\RequestCookies;
use Nimbus\Session\Session;
use Nimbus\Session\SessionConfig;
use Nimbus\Session\SessionPayload;
use Nimbus\Session\SessionStore;
use Nimbus\Validation\ValidationException;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Loads the session named by the session cookie, registers it in the request scope and
 * saves it after the handler. A session is only written (and its cookie sent) when it
 * already existed or the request changed it, so API calls that never touch it cost nothing.
 *
 * A ValidationException on a request that does not accept JSON becomes a redirect back
 * with the errors and the old input flashed, as HTML forms expect.
 */
final readonly class StartSessionMiddleware implements MiddlewareInterface
{
    public function __construct(
        private Container $container,
        private SessionStore $store,
        private SessionConfig $config = new SessionConfig(),
        private bool $redirectValidationErrors = true,
    ) {
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $cookies = $this->container->scopedValue(RequestCookies::class);
        $id = ($cookies instanceof RequestCookies ? $cookies : RequestCookies::fromRequest($request))->get($this->config->cookie);
        $data = $id !== null && SessionPayload::validId($id) ? $this->store->read($id) : null;
        $session = $id !== null && $data !== null ? new Session($id, $data, true) : new Session(Session::newId());
        $this->container->scopedInstance(Session::class, $session);
        Session::enter($session);
        try {
            $response = $next->handle($request, $response);
        } catch (ValidationException $invalid) {
            if (!$this->redirectValidationErrors || self::wantsJson($request)) {
                $this->save($session, $response);
                throw $invalid;
            }
            $session->flashErrors($invalid->errors)->flashInput(FormData::parse($request));
            $response = Redirect::back($request)->toResponse($request, $response);
        } catch (\Throwable $error) {
            $this->save($session, $response);
            throw $error;
        } finally {
            Session::enter(null);
        }
        $this->save($session, $response);
        return $response;
    }

    private function save(Session $session, Response $response): void
    {
        $session->ageFlashData();
        if (!$session->existed() && !$session->isDirty()) {
            return;
        }
        $previous = $session->previousId();
        if ($previous !== null) {
            $this->store->destroy($previous);
        }
        $this->store->write($session->id(), $session->all(), $this->config->lifetime);
        $response->addHeader('set-cookie', (new Cookie(
            $this->config->cookie,
            $session->id(),
            $this->config->expireOnClose ? 0 : $this->config->lifetime,
            $this->config->path,
            $this->config->domain,
            $this->config->secure,
            true,
            $this->config->sameSite,
        ))->toHeader());
        [$chances, $outOf] = $this->config->lottery;
        if ($chances > 0 && random_int(1, $outOf) <= $chances) {
            $this->store->gc();
        }
    }

    private static function wantsJson(Request $request): bool
    {
        $accept = strtolower($request->getHeader('accept') ?? '');
        return str_contains($accept, '/json') || str_contains($accept, '+json')
            || str_contains(strtolower($request->getHeader('content-type') ?? ''), 'json');
    }
}
