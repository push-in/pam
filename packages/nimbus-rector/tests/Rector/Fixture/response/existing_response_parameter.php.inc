<?php

namespace App\Http\Controllers;

use Pam\Http\Response;

final class PingController
{
    public function show(Response $res)
    {
        return response()->json(['message' => 'pong']);
    }
}
?>
-----
<?php

namespace App\Http\Controllers;

use Pam\Http\Response;

final class PingController
{
    public function show(Response $res)
    {
        return $res->json(['message' => 'pong']);
    }
}
?>
