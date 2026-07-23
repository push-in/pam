<?php

declare(strict_types=1);

use Pam\Compatibility\Probe;

echo json_encode(Probe::packages(), JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES), PHP_EOL;
