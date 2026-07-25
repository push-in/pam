<?php

declare(strict_types=1);

namespace Pam\Laravel\Http\Middleware;

use Closure;
use Illuminate\Http\Request;
use Pam\Laravel\Services\ObservabilityRegistry;
use Pam\Laravel\Services\StateGuard;
use RuntimeException;
use Symfony\Component\HttpFoundation\Response;

final readonly class GuardPersistentState
{
    public function __construct(
        private StateGuard $guard,
        private ObservabilityRegistry $observability,
    ) {
    }

    public function handle(Request $request, Closure $next): Response
    {
        $this->guard->begin();

        try {
            return $next($request);
        } finally {
            $violations = $this->guard->restore();
            foreach ($violations as $violation) {
                $this->observability->stateViolation('transaction', $violation);
            }

            if ($violations !== [] && (bool) config('pam.state_guard.strict', false)) {
                throw new RuntimeException(implode(' ', $violations));
            }
        }
    }
}
