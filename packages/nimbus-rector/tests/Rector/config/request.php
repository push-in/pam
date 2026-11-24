<?php

declare(strict_types=1);

use Nimbus\Rector\Rector\LaravelRequestCallToNimbusRector;
use Rector\Config\RectorConfig;

return RectorConfig::configure()->withRules([LaravelRequestCallToNimbusRector::class]);
