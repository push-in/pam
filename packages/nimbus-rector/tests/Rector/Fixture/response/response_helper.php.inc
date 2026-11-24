<?php

namespace App\Http\Controllers;

use Illuminate\Http\Request;

final class OrderController
{
    public function show(Request $request)
    {
        if ($request->route('id') === '0') {
            return response()->noContent();
        }
        return response()->json(['id' => 1], 201);
    }

    public function withHeaders()
    {
        return response()->json([], 200, ['x-a' => '1']);
    }

    public function untouched()
    {
        return ['id' => 1];
    }
}
?>
-----
<?php

namespace App\Http\Controllers;

use Illuminate\Http\Request;

final class OrderController
{
    public function show(Request $request, \Pam\Http\Response $response)
    {
        if ($request->route('id') === '0') {
            return $response->status(204);
        }
        return $response->json(['id' => 1], 201);
    }

    public function withHeaders()
    {
        return response()->json([], 200, ['x-a' => '1']);
    }

    public function untouched()
    {
        return ['id' => 1];
    }
}
?>
