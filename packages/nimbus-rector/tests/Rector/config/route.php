<?php

declare(strict_types=1);

use Nimbus\Rector\Rector\RouteFacadeToRouterRector;
use Rector\Config\RectorConfig;

return RectorConfig::configure()->withRules([RouteFacadeToRouterRector::class]);
