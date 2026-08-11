<?php

declare(strict_types=1);

namespace Pam\Octane;

use Illuminate\Http\Request as IlluminateRequest;
use Pam\Http\Request as PamRequest;
use Symfony\Component\HttpFoundation\Request as SymfonyRequest;

final class PamRequestMarshaller
{
    public function marshal(PamRequest $request): IlluminateRequest
    {
        // PAM has already populated PHP's request environment before invoking
        // the Octane bridge. Capturing it directly avoids rebuilding a second
        // Symfony request and follows the same fast path used by FrankenPHP.
        $captured = $this->capture();
        if ($request->body() === '' || $captured->getContent() !== '') {
            return $captured;
        }

        // Unit tests and third-party embedders may not expose the body through
        // php://input. Keep a compatibility fallback off the native hot path.
        return IlluminateRequest::createFromBase(new SymfonyRequest(
            query: $request->query(),
            request: $_POST,
            cookies: $_COOKIE,
            files: $_FILES,
            server: $_SERVER,
            content: $request->body(),
        ));
    }

    public function capture(): IlluminateRequest
    {
        return IlluminateRequest::capture();
    }
}
