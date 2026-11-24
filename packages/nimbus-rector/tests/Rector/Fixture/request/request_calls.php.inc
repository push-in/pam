<?php

namespace App\Http\Controllers;

use Illuminate\Http\Request;

final class OrderController
{
    public function index(Request $request)
    {
        $page = $request->query('page', 1);
        $all = $request->query();
        $trace = $request->header('x-trace-id');
        $headers = $request->header();
        $name = $request->input('customer.name', 'guest');
        $payload = $request->all();
        $hasItems = $request->has(['items', 'total']);
        $filled = $request->filled('coupon');
        $only = $request->only('total', 'currency');
        $token = $request->bearerToken();
        $raw = $request->getContent();
        $method = $request->method();
        $sku = $request->json('items.0.sku');
        $id = $request->route('id');
    }

    public function other(\stdClass $request)
    {
        return $request->input('name');
    }
}
?>
-----
<?php

namespace App\Http\Controllers;

use Illuminate\Http\Request;

final class OrderController
{
    public function index(Request $request)
    {
        $page = $request->getQuery('page', 1);
        $all = $request->query();
        $trace = $request->getHeader('x-trace-id');
        $headers = $request->headers();
        $name = \Nimbus\Http\RequestData::input($request, 'customer.name', 'guest');
        $payload = \Nimbus\Http\RequestData::all($request);
        $hasItems = \Nimbus\Http\RequestData::has($request, ['items', 'total']);
        $filled = \Nimbus\Http\RequestData::filled($request, 'coupon');
        $only = \Nimbus\Http\RequestData::only($request, 'total', 'currency');
        $token = \Nimbus\Http\RequestData::bearerToken($request);
        $raw = $request->body();
        $method = $request->method;
        $sku = \Nimbus\Http\RequestData::input($request, 'items.0.sku');
        $id = $request->route('id');
    }

    public function other(\stdClass $request)
    {
        return $request->input('name');
    }
}
?>
