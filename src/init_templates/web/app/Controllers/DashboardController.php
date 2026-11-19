<?php

declare(strict_types=1);

namespace App\Controllers;

use Nimbus\Auth\Principal;
use Nimbus\View\View;

final readonly class DashboardController
{
    public function show(Principal $user): View
    {
        return view('dashboard', ['user' => $user]);
    }
}
