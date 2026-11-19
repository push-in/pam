<?php

declare(strict_types=1);

namespace Nimbus\Admin;

use Nimbus\Auth\AuthContext;
use Nimbus\Auth\Authenticator;
use Nimbus\Auth\Gate;
use Nimbus\Auth\Principal;
use Nimbus\Container\Container;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Nimbus\Session\Session;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * @internal Signs the admin user in (or sends guests to the login page), requires the
 * `viewAdmin` ability and renders 403/404 errors as admin pages for browsers.
 */
final readonly class AdminMiddleware implements MiddlewareInterface
{
    public function __construct(
        private AdminPanel $panel,
        private Container $container,
    ) {
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        if (!$this->container->scopedValue(Session::class) instanceof Session) {
            throw new \LogicException('The admin panel requires Nimbus\\Session\\SessionServiceProvider.');
        }
        $authenticator = $this->container->has(Authenticator::class) ? $this->container->get(Authenticator::class) : null;
        if (!$authenticator instanceof Authenticator) {
            throw new \LogicException('The admin panel requires an Auth\\Authenticator such as SessionAuthenticator.');
        }
        $principal = $authenticator->authenticate($request);
        if ($principal === null) {
            $login = $this->panel->login();
            if ($login !== null && $request->method === 'GET') {
                $target = $request->path . (($query = http_build_query($request->query())) !== '' ? '?' . $query : '');
                return $response->status(302)
                    ->header('location', $login . (str_contains($login, '?') ? '&' : '?') . 'redirect=' . rawurlencode($target))
                    ->send(null);
            }
            throw new HttpException(401, ProblemCode::Unauthenticated, 'Authentication is required.');
        }
        $this->container
            ->scopedInstance(Principal::class, $principal)
            ->scopedInstance(AuthContext::class, new AuthContext($principal));
        try {
            $gate = $this->container->get(Gate::class);
            if (!$gate instanceof Gate || !$gate->check($principal, 'viewAdmin')) {
                throw new HttpException(403, ProblemCode::Forbidden, 'This action is not authorized.');
            }
            $response = $next->handle($request, $response);
        } catch (HttpException $error) {
            if (!in_array($error->status, [403, 404], true) || !self::wantsHtml($request)) {
                throw $error;
            }
            $html = $this->panel->views()->render('error', [
                'admin' => $this->panel,
                't' => $this->panel->text(...),
                'status' => $error->status,
                'message' => $this->panel->text($error->status === 403 ? 'forbidden' : 'not_found'),
                'navigation' => [],
                'current' => null,
                'flash' => null,
            ]);
            $response = $response->status($error->status)->header('content-type', 'text/html; charset=utf-8')->send($html);
        }
        return $response
            ->header('cache-control', 'no-store, private')
            ->header('x-frame-options', 'DENY')
            ->header('referrer-policy', 'same-origin')
            ->header('content-security-policy', "default-src 'self'; img-src 'self' data:; form-action 'self'; frame-ancestors 'none'; base-uri 'none'");
    }

    private static function wantsHtml(Request $request): bool
    {
        $accept = strtolower($request->getHeader('accept') ?? '');
        return $accept === '' || str_contains($accept, 'text/html') || str_contains($accept, '*/*');
    }
}
