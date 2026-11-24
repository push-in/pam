<?php

namespace App\Http\Middleware;

use Closure;
use Illuminate\Http\Request;

final class EnsureActive
{
    public function handle(Request $request, Closure $next)
    {
        if ($request->user() === null) {
            return response()->json(['message' => 'Inactive.'], 402);
        }
        return $next($request);
    }
}
?>
