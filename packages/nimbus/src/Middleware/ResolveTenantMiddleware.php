<?php

declare(strict_types=1);

namespace Nimbus\Middleware;

use Nimbus\Container\Container;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Nimbus\Tenancy\Tenant;
use Nimbus\Tenancy\TenantContext;
use Nimbus\Tenancy\TenantResolver;
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

