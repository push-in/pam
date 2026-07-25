<?php

declare(strict_types=1);

namespace Tests;

use Illuminate\Foundation\Testing\TestCase as BaseTestCase;
use Illuminate\Contracts\Console\Kernel;

abstract class TestCase extends BaseTestCase
{
    public function createApplication(): \Illuminate\Foundation\Application
    {
        $application = require __DIR__.'/../bootstrap/app.php';
        $application->make(Kernel::class)->bootstrap();

        return $application;
    }
}
