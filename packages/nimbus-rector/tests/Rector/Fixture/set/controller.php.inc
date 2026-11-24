<?php

namespace App\Http\Controllers;

use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\Http;

final class QuoteController
{
    public function show(Request $request): JsonResponse
    {
        $symbol = $request->input('symbol', 'BRL');
        if ($request->bearerToken() === null) {
            abort(401);
        }
        $quote = Cache::remember("quote:{$symbol}", 30, fn () => Http::timeout(2)->get('https://quotes.test/' . $symbol)->json());

        return response()->json(['symbol' => $symbol, 'quote' => $quote]);
    }
}
?>
-----
<?php

namespace App\Http\Controllers;

use Pam\Http\Request;
use Pam\Http\Response;
use Nimbus\Http\RequestData;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Nimbus\Cache\Cache;
use Nimbus\Http\Client\Http;

final class QuoteController
{
    public function show(Request $request, Response $response): Response
    {
        $symbol = RequestData::input($request, 'symbol', 'BRL');
        if (RequestData::bearerToken($request) === null) {
            throw new HttpException(401, ProblemCode::Unauthenticated, 'Unauthenticated.');
        }
        $quote = Cache::remember("quote:{$symbol}", 30, fn () => Http::timeout(2)->get('https://quotes.test/' . $symbol)->json());

        return $response->json(['symbol' => $symbol, 'quote' => $quote]);
    }
}
?>
