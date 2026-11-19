<?php

declare(strict_types=1);

namespace Nimbus\Middleware;

use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;
use Nimbus\Auth\AuthContext;
use Nimbus\Auth\Gate;
use Nimbus\Container\Container;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;

final readonly class AuthorizeMiddleware implements MiddlewareInterface
{
    /**
     * @param ?string $subject A route parameter (`product`) or entity class whose bound entity is
     *        checked by the Gate's policy; a class without a matching parameter is passed as is.
     */
    public function __construct(
        private Container $container,
        private string $ability,
        private ?string $subject = null,
    ) {
        if ($ability === '' || $subject === '') {
            throw new \InvalidArgumentException('Authorization ability cannot be empty.');
        }
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $auth = $this->container->get(AuthContext::class);
        if (!$auth instanceof AuthContext) {
            throw new HttpException(403, ProblemCode::Forbidden, 'This action is not authorized.');
        }
        $gate = $this->container->has(Gate::class) ? $this->container->get(Gate::class) : null;
        $allowed = $gate instanceof Gate
            ? $gate->check($auth->principal, $this->ability, $this->arguments($request, $gate))
            : $this->subject === null && $auth->principal->can($this->ability);
        if (!$allowed) {
            throw new HttpException(403, ProblemCode::Forbidden, 'This action is not authorized.');
        }
        return $next->handle($request, $response);
    }

    /** @return list<mixed> */
    private function arguments(Request $request, Gate $gate): array
    {
        if ($this->subject === null) {
            return [];
        }
        if (class_exists($this->subject)) {
            $class = $this->subject;
            $parameter = lcfirst(substr($class, (int) strrpos('\\' . $class, '\\')));
        } else {
            $parameter = $this->subject;
            $class = $gate->entityForParameter($parameter);
        }
        $value = $request->route($parameter);
        if ($value === null) {
            return $class === null ? [] : [$class];
        }
        return [$class === null ? $value : $this->container->resolveRouteBinding($class, $value)];
    }
}
