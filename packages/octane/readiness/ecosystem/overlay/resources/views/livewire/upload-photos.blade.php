<div>
    <input id="photos" type="file" wire:model="photos" multiple>
    <button id="save" wire:click="save">Save</button>
    <ul id="stored">@foreach ($stored as $line)<li>{{ $line }}</li>@endforeach</ul>
</div>
