<?php

declare(strict_types=1);

namespace Nimbus\Profiler;

enum ProfilerMode: int
{
    case Disabled = 1;
    case Development = 2;
    case Testing = 3;
}
