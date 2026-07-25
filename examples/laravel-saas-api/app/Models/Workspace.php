<?php

declare(strict_types=1);

namespace App\Models;

use App\Enums\WorkspaceStatus;
use Illuminate\Database\Eloquent\Factories\HasFactory;
use Illuminate\Database\Eloquent\Model;

final class Workspace extends Model
{
    use HasFactory;

    /** @var list<string> */
    protected $fillable = ['name', 'status'];

    /** @return array<string, string> */
    protected function casts(): array
    {
        return ['status' => WorkspaceStatus::class];
    }
}
