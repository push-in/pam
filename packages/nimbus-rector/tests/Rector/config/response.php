<?php

declare(strict_types=1);

use Nimbus\Rector\Rector\ResponseHelperToResponseParameterRector;
use Rector\Config\RectorConfig;

return RectorConfig::configure()->withRules([ResponseHelperToResponseParameterRector::class]);
