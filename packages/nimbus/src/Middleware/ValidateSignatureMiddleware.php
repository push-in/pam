<?php

declare(strict_types=1);

namespace Nimbus\Middleware;

use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Nimbus\Security\UrlSignatureState;
use Nimbus\Security\UrlSigner;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

/** Rejects requests whose URL was not signed by `UrlSigner` or whose signature expired. */
final readonly class ValidateSignatureMiddleware implements MiddlewareInterface
{
    public function __construct(private UrlSigner $signer)
    {
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        return match ($this->signer->check($request->path, $request->query())) {
            UrlSignatureState::Valid => $next->handle($request, $response),
            UrlSignatureState::Expired => throw new HttpException(
                403,
                ProblemCode::InvalidSignature,
                'The signed URL has expired.',
            ),
            UrlSignatureState::Invalid => throw new HttpException(
                403,
                ProblemCode::InvalidSignature,
                'The URL signature is invalid.',
            ),
        };
    }
}
