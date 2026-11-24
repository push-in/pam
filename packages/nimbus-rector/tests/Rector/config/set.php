<?php

declare(strict_types=1);

use Nimbus\Rector\Set\NimbusSetList;
use Rector\Config\RectorConfig;

return RectorConfig::configure()->withSets([NimbusSetList::LARAVEL_TO_NIMBUS])
    ->withImportNames(importShortClasses: false, removeUnusedImports: true);
