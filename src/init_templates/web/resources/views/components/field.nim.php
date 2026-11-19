@props(['name', 'label', 'type' => 'text', 'keep' => true])
<label>
    {{ $label }}
    <input name="{{ $name }}" type="{{ $type }}" @if ($keep) value="{{ old($name) }}" @endif {{ $attributes }}>
    @error($name)<span class="error">{{ $message }}</span>@enderror
</label>
