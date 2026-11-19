<?php

declare(strict_types=1);

namespace Tests;

use Nimbus\App;
use Nimbus\Notifications\NotificationDispatcher;
use Nimbus\Notifications\NotificationFake;

abstract class TestCase extends \Nimbus\Testing\TestCase
{
    protected NotificationFake $notifications;

    protected function createApplication(): App
    {
        $app = require dirname(__DIR__) . '/bootstrap/app.php';
        $this->notifications = new NotificationFake();
        $app->container()->instance(NotificationDispatcher::class, $this->notifications);

        return $app;
    }

    /** A client that keeps the session cookie and sends the CSRF token with every form. */
    protected function browser(): Browser
    {
        return new Browser($this->client());
    }
}
