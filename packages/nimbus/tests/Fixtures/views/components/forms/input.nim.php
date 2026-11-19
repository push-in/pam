@props(['name', 'label' => null])
<label>{{ $label ?? $name }}<input name="{{ $name }}" value="{{ old($name) }}" {{ $attributes }}></label>
@error($name)<span class="error">{{ $message }}</span>@enderror
