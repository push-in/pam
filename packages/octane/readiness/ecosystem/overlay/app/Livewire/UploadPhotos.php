<?php

namespace App\Livewire;

use Livewire\Component;
use Livewire\WithFileUploads;

class UploadPhotos extends Component
{
    use WithFileUploads;

    /** @var array<int, \Livewire\Features\SupportFileUploads\TemporaryUploadedFile> */
    public array $photos = [];

    public array $stored = [];

    public function save(): void
    {
        $this->validate(['photos.*' => 'file|max:4096']);
        $this->stored = collect($this->photos)
            ->map(fn ($photo) => $photo->getClientOriginalName().':'.$photo->getSize().':'.basename($photo->store('readiness', 'public')))
            ->all();
    }

    public function render()
    {
        return view('livewire.upload-photos');
    }
}
