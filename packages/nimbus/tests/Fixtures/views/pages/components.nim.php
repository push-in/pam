<x-layout title="Components">
<x-alert type="error" class="mb-4" :dismissible="true" data-id="{{ $id }}">
<x-slot:title>Heads up</x-slot:title>
Body <b>{{ $text }}</b>
<x-slot:footer class="small">Footer</x-slot>
</x-alert>
<x-forms.input name="email" label="E-mail" type="email" />
<x-badge :count="3" color="red" />
</x-layout>
