<?php

function show($order, $user)
{
    abort(404, 'Order not found.');
    abort(403);
    abort_if($order->closed, 409, 'Order is closed.');
    abort_unless($user->isAdmin(), 403);
    abort(402, 'Payment required.');
    abort($status);
}
?>
-----
<?php

function show($order, $user)
{
    throw new \Nimbus\Http\HttpException(404, \Nimbus\Http\ProblemCode::NotFound, 'Order not found.');
    throw new \Nimbus\Http\HttpException(403, \Nimbus\Http\ProblemCode::Forbidden, 'Forbidden.');
    if ($order->closed) {
        throw new \Nimbus\Http\HttpException(409, \Nimbus\Http\ProblemCode::Conflict, 'Order is closed.');
    }
    if (!$user->isAdmin()) {
        throw new \Nimbus\Http\HttpException(403, \Nimbus\Http\ProblemCode::Forbidden, 'Forbidden.');
    }
    abort(402, 'Payment required.');
    abort($status);
}
?>
