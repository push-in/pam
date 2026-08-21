<?php

declare(strict_types=1);

namespace Pam\Api\Middleware;

use Pam\Api\Transactions\TransactionManager;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class TransactionalMiddleware implements MiddlewareInterface
{
    public function __construct(private TransactionManager $transactions)
    {
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $result = $this->transactions->transaction(
            static fn (): Response => $next->handle($request, $response),
        );
        if (!$result instanceof Response) {
            throw new \UnexpectedValueException('Transaction manager must preserve the response result.');
        }
        return $result;
    }
}

