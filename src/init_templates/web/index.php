<?php

declare(strict_types=1);

/** @var \Nimbus\App $app */
$app = require __DIR__ . '/bootstrap/app.php';
$app->listen((int) (getenv('PAM_PORT') ?: 3000));
