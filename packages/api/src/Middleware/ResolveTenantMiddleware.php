<?php

declare(strict_types=1);

namespace Pam\Api\Middleware;

use Pam\Api\Container\Container;
use Pam\Api\Http\HttpException;
use Pam\Api\Http\ProblemCode;
use Pam\Api\Tenancy\Tenant;
use Pam\Api\Tenancy\TenantContext;
use Pam\Api\Tenancy\TenantResolver;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class ResolveTenantMiddleware implements MiddlewareInterface
{
    public function __construct(
        private TenantResolver $resolver,
        private Container $container,
        private bool $required = true,
    ) {
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $tenant = $this->resolver->resolve($request);
        if ($tenant === null) {
            if ($this->required) {
                throw new HttpException(404, ProblemCode::NotFound, 'Tenant was not found.');
            }
            return $next->handle($request, $response);
        }
        $this->container
            ->scopedInstance(Tenant::class, $tenant)
            ->scopedInstance(TenantContext::class, new TenantContext($tenant));
        return $next->handle($request, $response);
    }
}

