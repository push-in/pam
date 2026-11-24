<?php

declare(strict_types=1);

use Nimbus\Rector\Rector\AbortToHttpExceptionRector;
use Rector\Config\RectorConfig;

return RectorConfig::configure()->withRules([AbortToHttpExceptionRector::class]);
